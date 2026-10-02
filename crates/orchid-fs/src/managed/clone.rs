//! Block-clone a source range into a chunk file.
//!
//! Windows asks for `FSCTL_DUPLICATE_EXTENTS_TO_FILE`. Linux uses
//! `copy_file_range`, which clones on filesystems that support it. Failure
//! deletes `dest` so the caller can write the bytes. Success is reported
//! only when `dest` matches `expected`.

use std::path::Path;

/// Create `dest` as a shared copy of `src` at `src_offset` for `expected`.
#[must_use]
pub fn try_clone_range(src: &Path, src_offset: u64, dest: &Path, expected: &[u8]) -> bool {
    if expected.is_empty() {
        return false;
    }
    let len = expected.len() as u64;
    if !clone_range(src, src_offset, dest, len) {
        let _ = std::fs::remove_file(dest);
        return false;
    }
    match std::fs::read(dest) {
        Ok(got) if got == expected => true,
        _ => {
            let _ = std::fs::remove_file(dest);
            false
        }
    }
}

fn clone_range(src: &Path, src_offset: u64, dest: &Path, len: u64) -> bool {
    #[cfg(windows)]
    {
        clone_range_windows(src, src_offset, dest, len)
    }
    #[cfg(target_os = "linux")]
    {
        clone_range_linux(src, src_offset, dest, len)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (src, src_offset, dest, len);
        false
    }
}

#[cfg(windows)]
fn clone_range_windows(src: &Path, src_offset: u64, dest: &Path, len: u64) -> bool {
    use std::os::windows::io::AsRawHandle;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Ioctl::{DUPLICATE_EXTENTS_DATA, FSCTL_DUPLICATE_EXTENTS_TO_FILE};
    use windows::Win32::System::IO::DeviceIoControl;

    let Ok(src_off) = i64::try_from(src_offset) else {
        return false;
    };
    let Ok(byte_count) = i64::try_from(len) else {
        return false;
    };
    let src_file = match std::fs::File::open(src) {
        Ok(file) => file,
        Err(_) => return false,
    };
    if let Some(parent) = dest.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let dest_file = match std::fs::File::create(dest) {
        Ok(file) => file,
        Err(_) => return false,
    };
    if dest_file.set_len(len).is_err() {
        return false;
    }
    let data = DUPLICATE_EXTENTS_DATA {
        FileHandle: HANDLE(src_file.as_raw_handle()),
        SourceFileOffset: src_off,
        TargetFileOffset: 0,
        ByteCount: byte_count,
    };
    let Ok(size) = u32::try_from(std::mem::size_of::<DUPLICATE_EXTENTS_DATA>()) else {
        return false;
    };
    // SAFETY: both handles belong to files this function opened, and `data`
    // is the input struct documented for this ioctl. A failure is returned.
    let ok = unsafe {
        DeviceIoControl(
            HANDLE(dest_file.as_raw_handle()),
            FSCTL_DUPLICATE_EXTENTS_TO_FILE,
            Some((&raw const data).cast()),
            size,
            None,
            0,
            None,
            None,
        )
    };
    ok.is_ok() && dest_file.sync_all().is_ok()
}

#[cfg(target_os = "linux")]
fn clone_range_linux(src: &Path, src_offset: u64, dest: &Path, len: u64) -> bool {
    use std::os::unix::io::AsRawFd;

    let Ok(off_in_start) = i64::try_from(src_offset) else {
        return false;
    };
    let src_file = match std::fs::File::open(src) {
        Ok(file) => file,
        Err(_) => return false,
    };
    if let Some(parent) = dest.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let dest_file = match std::fs::File::create(dest) {
        Ok(file) => file,
        Err(_) => return false,
    };
    if dest_file.set_len(len).is_err() {
        return false;
    }
    let mut off_in = off_in_start;
    let mut off_out = 0i64;
    // SAFETY: the descriptors belong to files this function opened, and the
    // offsets point at that stack memory for the duration of the call.
    let copied = unsafe {
        libc::copy_file_range(
            src_file.as_raw_fd(),
            &raw mut off_in,
            dest_file.as_raw_fd(),
            &raw mut off_out,
            len as usize,
            0,
        )
    };
    copied == isize::try_from(len).unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clone_range_matches_or_refuses() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        let dest = dir.path().join("nested").join("dest.bin");
        let bytes = b"managed-ingest-clone";
        std::fs::write(&src, bytes).unwrap();
        let cloned = try_clone_range(&src, 0, &dest, bytes);
        if cloned {
            assert_eq!(std::fs::read(&dest).unwrap(), bytes);
        } else {
            assert!(!dest.exists());
        }
    }
}
