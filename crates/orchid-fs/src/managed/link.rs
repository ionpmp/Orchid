//! Replace one managed file with a hard link to an identical sibling.
//!
//! The directory entry changes. The bytes do not. An in-place write updates
//! every name. A save that writes a new file and renames it over the path
//! breaks the link; the next ingest stores that file on its own.

use std::path::{Path, PathBuf};

/// Share `keep`'s bytes at `replace`.
///
/// # Errors
///
/// Returns an I/O error when the files are on different volumes, are not
/// regular files, or the link cannot be created. On failure `replace` is
/// put back when the temporary rename succeeded.
pub fn link_duplicate(keep: &Path, replace: &Path) -> std::io::Result<bool> {
    let keep_meta = std::fs::metadata(keep)?;
    let replace_meta = std::fs::metadata(replace)?;
    if !keep_meta.is_file() || !replace_meta.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "hardlink ingest needs two files",
        ));
    }
    if paths_share_data(keep, replace) {
        return Ok(false);
    }
    if !same_volume(keep, replace) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "hardlink ingest needs one volume",
        ));
    }
    let tmp = temp_link_path(replace);
    std::fs::rename(replace, &tmp)?;
    match std::fs::hard_link(keep, replace) {
        Ok(()) => {
            let _ = std::fs::remove_file(&tmp);
            Ok(true)
        }
        Err(e) => {
            if let Err(restore) = std::fs::rename(&tmp, replace) {
                tracing::error!(
                    error = %restore,
                    tmp = %tmp.display(),
                    "hardlink ingest could not restore the original file"
                );
            }
            Err(e)
        }
    }
}

fn temp_link_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    path.with_file_name(format!(".{name}.orchid-link"))
}

/// True when both paths are the same file (a hard link pair, or one path).
#[must_use]
pub fn paths_share_data(a: &Path, b: &Path) -> bool {
    match (file_id(a), file_id(b)) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileId {
    volume: u64,
    index: u64,
}

fn file_id(path: &Path) -> std::io::Result<FileId> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let file = std::fs::File::open(path)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `file` is open and `info` is a writable struct of the size
        // this call expects. The handle stays valid until `file` drops.
        unsafe {
            GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
        }
        let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
        Ok(FileId {
            volume: u64::from(info.dwVolumeSerialNumber),
            index,
        })
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path)?;
        Ok(FileId {
            volume: meta.dev(),
            index: meta.ino(),
        })
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "file identity",
        ))
    }
}

fn same_volume(a: &Path, b: &Path) -> bool {
    match (file_id(a), file_id(b)) {
        (Ok(left), Ok(right)) => left.volume == right.volume,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_names_share_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join("keep.txt");
        let replace = dir.path().join("replace.txt");
        std::fs::write(&keep, b"same-bytes").unwrap();
        std::fs::write(&replace, b"same-bytes").unwrap();
        assert!(link_duplicate(&keep, &replace).unwrap());
        assert!(paths_share_data(&keep, &replace));
        assert!(!link_duplicate(&keep, &replace).unwrap());
    }
}
