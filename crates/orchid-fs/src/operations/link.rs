//! Create symbolic links, hard links, and directory junctions.

use std::path::Path;

use crate::error::{FsError, Result};
use crate::path::FsPath;

/// Create a symbolic link at `link` pointing at `target`.
///
/// On Windows this uses `CreateSymbolicLinkW` (file or directory).
///
/// # Errors
///
/// Returns [`FsError::AlreadyExists`] if `link` exists, or an I/O error from
/// the OS (Windows may require Developer Mode / elevation).
pub async fn create_symlink(link: &FsPath, target: &FsPath) -> Result<()> {
    let link_os = link.to_local()?;
    let target_os = target.to_local()?;
    if tokio::fs::try_exists(&link_os).await.unwrap_or(false) {
        return Err(FsError::AlreadyExists(link.to_string()));
    }
    let is_dir = tokio::fs::metadata(&target_os)
        .await
        .map(|m| m.is_dir())
        .unwrap_or(false);
    tokio::task::spawn_blocking(move || symlink_blocking(&link_os, &target_os, is_dir))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Create a hard link at `link` for the file `target`.
///
/// # Errors
///
/// Fails when `target` is not a file, or the OS refuses the link (cross-volume).
pub async fn create_hard_link(link: &FsPath, target: &FsPath) -> Result<()> {
    let link_os = link.to_local()?;
    let target_os = target.to_local()?;
    if tokio::fs::try_exists(&link_os).await.unwrap_or(false) {
        return Err(FsError::AlreadyExists(link.to_string()));
    }
    tokio::task::spawn_blocking(move || std::fs::hard_link(&target_os, &link_os))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
        .map_err(FsError::Io)
}

/// Create a directory junction (Windows) or a directory symlink (Unix).
///
/// # Errors
///
/// Fails when `target` is not a directory or the OS refuses the link.
pub async fn create_junction(link: &FsPath, target: &FsPath) -> Result<()> {
    let link_os = link.to_local()?;
    let target_os = target.to_local()?;
    if tokio::fs::try_exists(&link_os).await.unwrap_or(false) {
        return Err(FsError::AlreadyExists(link.to_string()));
    }
    let meta = tokio::fs::metadata(&target_os).await.map_err(FsError::Io)?;
    if !meta.is_dir() {
        return Err(FsError::InvalidPath {
            reason: "junction target must be a directory".into(),
        });
    }
    tokio::task::spawn_blocking(move || junction_blocking(&link_os, &target_os))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn symlink_blocking(link: &Path, target: &Path, is_dir: bool) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::{symlink_dir, symlink_file};
        if is_dir {
            symlink_dir(target, link).map_err(FsError::Io)
        } else {
            symlink_file(target, link).map_err(FsError::Io)
        }
    }
    #[cfg(unix)]
    {
        let _ = is_dir;
        std::os::unix::fs::symlink(target, link).map_err(FsError::Io)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (link, target, is_dir);
        Err(FsError::InvalidPath {
            reason: "symlinks are not supported on this platform".into(),
        })
    }
}

fn junction_blocking(link: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        // Junctions do not require elevation (unlike directory symlinks).
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link.to_string_lossy(),
                &target.to_string_lossy(),
            ])
            .status()
            .map_err(FsError::Io)?;
        if status.success() {
            Ok(())
        } else {
            Err(FsError::Io(std::io::Error::other(format!(
                "mklink /J failed with {status}"
            ))))
        }
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).map_err(FsError::Io)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = (link, target);
        Err(FsError::InvalidPath {
            reason: "junctions are not supported on this platform".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn hard_link_roundtrip() {
        let td = tempfile::tempdir().unwrap();
        let target = td.path().join("src.txt");
        let link = td.path().join("link.txt");
        std::fs::write(&target, b"payload").unwrap();
        create_hard_link(
            &FsPath::from_local(&link).unwrap(),
            &FsPath::from_local(&target).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&link).unwrap(), b"payload");
        std::fs::write(&target, b"changed").unwrap();
        assert_eq!(std::fs::read(&link).unwrap(), b"changed");
    }
}
