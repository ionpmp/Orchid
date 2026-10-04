//! Free-space overwrite. One pass of zeros is enough for a magnetic disk.
//! The filler file is deleted afterwards, including when the wipe is cancelled.
//! A reserved margin is left so the volume is not filled to the last byte.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Bytes left free on the volume. Matches the margin CCleaner keeps so the
/// system can still write while the wipe runs.
pub const RESERVE_BYTES: u64 = 64 * 1024 * 1024;

/// How many bytes to try to overwrite.
#[must_use]
pub fn wipe_budget(available: u64) -> u64 {
    available.saturating_sub(RESERVE_BYTES)
}

/// Pass count accepted by the widget: 1, 2, or 3.
#[must_use]
pub fn clamp_passes(passes: u8) -> u8 {
    passes.clamp(1, 3)
}

/// Drive-letter key (`c:`) or the full path when there is no drive letter.
#[must_use]
pub fn volume_key(path: &Path) -> String {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return text[..2].to_ascii_lowercase();
    }
    text.to_string()
}

/// Directory that can hold the filler file for `mount`.
///
/// A file anywhere on the volume consumes that volume's free space. When the
/// user's temp directory is on the same volume, it is writable without
/// administrator rights. Other volumes use the mount point itself.
#[must_use]
pub fn filler_directory(mount: &Path, temp: &Path) -> PathBuf {
    if volume_key(mount) == volume_key(temp) {
        temp.to_path_buf()
    } else {
        mount.to_path_buf()
    }
}

/// Outcome of [`wipe_free_space`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WipeReport {
    /// Zero bytes written across every finished or partial pass.
    pub written: u64,
    /// Passes that ran, including a pass stopped by a full disk.
    pub passes_finished: u8,
    /// True when `cancel` was set.
    pub cancelled: bool,
}

/// Overwrite up to `budget_bytes` of free space in `dir`, `passes` times.
///
/// `progress` receives the running total of bytes written. The filler file is
/// removed before this function returns. `sync_every_chunks == 0` skips
/// `sync_data` (used by tests).
///
/// # Errors
///
/// Returns an error when the filler file cannot be created, or when the first
/// write of a pass fails for a reason other than a full disk.
pub fn wipe_free_space(
    dir: &Path,
    budget_bytes: u64,
    passes: u8,
    chunk_bytes: usize,
    sync_every_chunks: u32,
    cancel: &AtomicBool,
    progress: &AtomicU64,
) -> Result<WipeReport, String> {
    let passes = clamp_passes(passes);
    let chunk_bytes = chunk_bytes.max(1);
    if budget_bytes == 0 {
        return Ok(WipeReport {
            written: 0,
            passes_finished: 0,
            cancelled: false,
        });
    }
    fs::create_dir_all(dir).map_err(|e| format!("create wipe directory: {e}"))?;
    let filler = dir.join("orchid-free-space.tmp");
    let zeros = vec![0u8; chunk_bytes];
    let mut written = 0u64;
    let mut cancelled = false;
    let mut passes_finished = 0u8;
    for _ in 0..passes {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        match write_pass(
            &filler,
            budget_bytes,
            &zeros,
            sync_every_chunks,
            cancel,
            &mut written,
            progress,
        ) {
            Ok(PassEnd::Cancelled) => {
                cancelled = true;
                passes_finished = passes_finished.saturating_add(1);
                break;
            }
            Ok(PassEnd::Done) => {
                passes_finished = passes_finished.saturating_add(1);
            }
            Err(err) => {
                let _ = fs::remove_file(&filler);
                return Err(err);
            }
        }
        let _ = fs::remove_file(&filler);
    }
    let _ = fs::remove_file(&filler);
    progress.store(written, Ordering::Relaxed);
    Ok(WipeReport {
        written,
        passes_finished,
        cancelled,
    })
}

enum PassEnd {
    Done,
    Cancelled,
}

fn write_pass(
    filler: &Path,
    budget: u64,
    zeros: &[u8],
    sync_every_chunks: u32,
    cancel: &AtomicBool,
    written: &mut u64,
    progress: &AtomicU64,
) -> Result<PassEnd, String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(filler)
        .map_err(|e| format!("create filler: {e}"))?;
    let mut remaining = budget;
    let mut since_sync = 0u32;
    let mut pass_written = 0u64;
    while remaining > 0 {
        if cancel.load(Ordering::Relaxed) {
            let _ = file.flush();
            return Ok(PassEnd::Cancelled);
        }
        let n = zeros.len().min(remaining as usize);
        if let Err(err) = file.write_all(&zeros[..n]) {
            if pass_written > 0 || is_disk_full(&err) {
                break;
            }
            return Err(format!("write filler: {err}"));
        }
        remaining -= n as u64;
        pass_written += n as u64;
        *written += n as u64;
        progress.store(*written, Ordering::Relaxed);
        since_sync += 1;
        if sync_every_chunks > 0 && since_sync >= sync_every_chunks {
            let _ = file.sync_data();
            since_sync = 0;
        }
    }
    let _ = file.flush();
    Ok(PassEnd::Done)
}

fn is_disk_full(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::StorageFull || err.raw_os_error() == Some(112)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_keeps_the_reserve() {
        assert_eq!(wipe_budget(RESERVE_BYTES), 0);
        assert_eq!(wipe_budget(RESERVE_BYTES + 10), 10);
        assert_eq!(wipe_budget(0), 0);
    }

    #[test]
    fn filler_uses_temp_on_the_same_volume() {
        let temp = Path::new(r"C:\Users\me\AppData\Local\Temp");
        let mount = Path::new(r"C:\");
        assert_eq!(filler_directory(mount, temp), temp);
        assert_eq!(filler_directory(Path::new(r"D:\"), temp), Path::new(r"D:\"));
    }

    #[test]
    fn wipe_writes_the_budget_and_deletes_the_filler() {
        let root = tempfile::tempdir().expect("temp");
        let cancel = AtomicBool::new(false);
        let progress = AtomicU64::new(0);
        let report =
            wipe_free_space(root.path(), 4096, 2, 1024, 0, &cancel, &progress).expect("wipe");
        assert_eq!(report.written, 8192);
        assert_eq!(report.passes_finished, 2);
        assert!(!report.cancelled);
        assert!(!root.path().join("orchid-free-space.tmp").exists());
        assert_eq!(progress.load(Ordering::Relaxed), 8192);
    }

    #[test]
    fn wipe_stops_when_cancelled() {
        let root = tempfile::tempdir().expect("temp");
        let cancel = AtomicBool::new(true);
        let progress = AtomicU64::new(0);
        let report =
            wipe_free_space(root.path(), 1_000_000, 1, 1024, 0, &cancel, &progress).expect("wipe");
        assert!(report.cancelled);
        assert_eq!(report.written, 0);
        assert!(!root.path().join("orchid-free-space.tmp").exists());
    }
}
