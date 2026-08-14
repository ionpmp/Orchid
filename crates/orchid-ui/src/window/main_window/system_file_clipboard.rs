//! System clipboard file lists (`CF_HDROP` + Preferred DropEffect).

use std::path::PathBuf;

use tracing::warn;

use orchid_fs::FsPath;
use orchid_widgets::builtin::file_manager::{ClipboardOperation, FileClipboard};

use crate::widgets::terminal::ArboardClipboard;

/// Local OS paths from an Orchid file-manager clipboard.
#[must_use]
pub(super) fn local_os_paths(paths: &[FsPath]) -> Vec<PathBuf> {
    paths.iter().filter_map(|p| p.to_local().ok()).collect()
}

/// Push the FM clipboard onto the OS clipboard so other apps can paste.
pub(super) fn push_file_clipboard(clip: &FileClipboard) {
    let paths = clip.paths();
    let local = local_os_paths(&paths);
    let cut = clip.operation() == ClipboardOperation::Cut;
    match ArboardClipboard::new() {
        Ok(cb) => {
            if local.is_empty() {
                let text = paths
                    .iter()
                    .map(FsPath::as_str)
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    if let Err(e) = cb.copy(&text) {
                        warn!(?e, "system clipboard text (remote paths)");
                    }
                }
                clip.note_os_files(0, false);
            } else if let Err(e) = cb.set_file_list(&local) {
                warn!(?e, "system clipboard file list");
                clip.note_os_files(0, false);
            } else {
                set_preferred_drop_effect(cut);
                clip.note_os_files(local.len(), cut);
            }
        }
        Err(e) => warn!(?e, "open system clipboard for file copy"),
    }
}

/// Read files offered by Explorer / other apps.
#[must_use]
pub(super) fn read_os_file_list() -> Option<(Vec<PathBuf>, bool)> {
    let cb = ArboardClipboard::new().ok()?;
    let paths = cb.get_file_list().ok().flatten()?;
    if paths.is_empty() {
        return None;
    }
    Some((paths, preferred_drop_effect_is_cut()))
}

/// Refresh the FM clipboard's "OS has files" flag (enables Paste).
pub(super) fn refresh_os_file_note(clip: &FileClipboard) {
    match read_os_file_list() {
        Some((paths, cut)) => clip.note_os_files(paths.len(), cut),
        None => clip.note_os_files(0, false),
    }
}

/// If the OS clipboard holds files, stage them on the FM clipboard for paste.
pub(super) fn ingest_os_files(clip: &FileClipboard) -> bool {
    let Some((os_paths, cut)) = read_os_file_list() else {
        return false;
    };
    let fps: Vec<FsPath> = os_paths
        .iter()
        .filter_map(|p| FsPath::from_local(p).ok())
        .collect();
    if fps.is_empty() {
        return false;
    }
    if cut {
        clip.cut(fps);
    } else {
        clip.copy(fps);
    }
    true
}

#[cfg(windows)]
fn set_preferred_drop_effect(cut: bool) {
    use windows::core::w;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    let effect: u32 = if cut { 2 } else { 1 };
    unsafe {
        let fmt = RegisterClipboardFormatW(w!("Preferred DropEffect"));
        if fmt == 0 {
            return;
        }
        if OpenClipboard(None).is_err() {
            return;
        }
        let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of::<u32>()) else {
            let _ = CloseClipboard();
            return;
        };
        let ptr = GlobalLock(mem);
        if ptr.is_null() {
            let _ = CloseClipboard();
            return;
        }
        ptr.cast::<u32>().write(effect);
        let _ = GlobalUnlock(mem);
        let _ = SetClipboardData(fmt, Some(HANDLE(mem.0)));
        let _ = CloseClipboard();
    }
}

#[cfg(not(windows))]
fn set_preferred_drop_effect(_cut: bool) {}

#[cfg(windows)]
fn preferred_drop_effect_is_cut() -> bool {
    use windows::core::w;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    unsafe {
        let fmt = RegisterClipboardFormatW(w!("Preferred DropEffect"));
        if fmt == 0 || OpenClipboard(None).is_err() {
            return false;
        }
        let handle = GetClipboardData(fmt).ok();
        let cut = handle
            .and_then(|h| {
                let mem = windows::Win32::Foundation::HGLOBAL(h.0);
                let ptr = GlobalLock(mem);
                if ptr.is_null() {
                    return None;
                }
                let effect = ptr.cast::<u32>().read();
                let _ = GlobalUnlock(mem);
                Some(effect == 2)
            })
            .unwrap_or(false);
        let _ = CloseClipboard();
        cut
    }
}

#[cfg(not(windows))]
fn preferred_drop_effect_is_cut() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_os_paths_skips_remote() {
        let local = FsPath::new("local:c:/Users/a.txt").unwrap();
        let remote = FsPath::new("sftp:host/a.txt").unwrap();
        let out = local_os_paths(&[local, remote]);
        assert_eq!(out.len(), 1);
        assert!(out[0].to_string_lossy().contains("Users"));
    }
}
