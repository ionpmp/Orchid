//! SMB folder shares (Windows Sharing tab).

use std::path::{Path, PathBuf};

use crate::error::{FsError, Result};
use crate::path::FsPath;

/// One SMB share that covers a local path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderShare {
    /// Share name (`Documents`, `C$`, …).
    pub name: String,
    /// Local folder the share points at.
    pub local_path: PathBuf,
    /// Optional comment.
    pub remark: String,
    /// `\\HOST\name` plus a relative suffix when the target is nested.
    pub unc: String,
    /// Connection limit; `None` means unlimited.
    pub max_uses: Option<u32>,
    /// Current connections.
    pub current_uses: u32,
    /// Hidden administrative share (`C$`, `ADMIN$`, …).
    pub administrative: bool,
    /// Share root equals the queried path (not a parent).
    pub exact: bool,
}

/// Whether `name` is a built-in administrative share that must not be deleted here.
#[must_use]
pub fn is_admin_share_name(name: &str) -> bool {
    let n = name.trim();
    n.eq_ignore_ascii_case("ADMIN$")
        || n.eq_ignore_ascii_case("IPC$")
        || n.eq_ignore_ascii_case("print$")
        || (n.len() == 2 && n.as_bytes()[1] == b'$' && n.as_bytes()[0].is_ascii_alphabetic())
}

/// Shares whose root is `path` or a parent of `path`.
///
/// # Errors
///
/// Non-local paths or OS enumeration failures.
pub async fn shares_for_path(path: &FsPath) -> Result<Vec<FolderShare>> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || shares_for_os_path(&os))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Create a disk share for a local folder.
///
/// # Errors
///
/// Non-folder targets, duplicate names, or access denied.
pub async fn add_folder_share(path: &FsPath, name: &str, remark: &str) -> Result<FolderShare> {
    let os = path.to_local()?;
    let name = name.trim().to_string();
    let remark = remark.trim().to_string();
    tokio::task::spawn_blocking(move || add_folder_share_os(&os, &name, &remark))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Remove a non-administrative share by name.
///
/// # Errors
///
/// Administrative names, missing shares, or access denied.
pub async fn remove_folder_share(name: &str) -> Result<()> {
    let name = name.trim().to_string();
    tokio::task::spawn_blocking(move || remove_folder_share_os(&name))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Open the Windows Properties dialog on the Sharing page.
///
/// # Errors
///
/// Non-local paths or Shell failure.
pub async fn open_sharing_tab(path: &FsPath) -> Result<()> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || open_sharing_tab_os(&os))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Exact non-admin share for this folder, if any.
#[must_use]
pub fn exact_user_share(shares: &[FolderShare]) -> Option<&FolderShare> {
    shares.iter().find(|s| s.exact && !s.administrative)
}

/// `true` when `share_root` is `target` or a parent of `target`.
#[must_use]
pub fn share_covers_path(share_root: &Path, target: &Path) -> bool {
    let root = normalize_os_path(share_root);
    let dest = normalize_os_path(target);
    dest == root || dest.starts_with(&format!("{root}\\"))
}

fn normalize_os_path(path: &Path) -> String {
    let raw = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\");
    let trimmed = raw
        .trim_start_matches(r"\\?\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    trimmed
}

fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "localhost".into())
}

fn unc_for(name: &str, share_root: &Path, target: &Path) -> String {
    let host = computer_name();
    let root = normalize_os_path(share_root);
    let dest = normalize_os_path(target);
    let suffix = dest
        .strip_prefix(&root)
        .unwrap_or("")
        .trim_start_matches('\\')
        .replace('\\', "/");
    if suffix.is_empty() {
        format!(r"\\{host}\{name}")
    } else {
        format!(r"\\{host}\{name}\{suffix}")
    }
}

fn map_share_status(code: u32) -> FsError {
    match code {
        5 => FsError::Io(std::io::Error::other("fm-error-share-denied")),
        2118 => FsError::Io(std::io::Error::other("fm-error-share-exists")),
        2310 | 2311 => FsError::NotFound("share".into()),
        other => FsError::Io(std::io::Error::from_raw_os_error(other as i32)),
    }
}

#[cfg(windows)]
fn shares_for_os_path(path: &Path) -> Result<Vec<FolderShare>> {
    let mut out = Vec::new();
    for mut s in list_disk_shares()? {
        if !share_covers_path(&s.local_path, path) {
            continue;
        }
        s.exact = normalize_os_path(&s.local_path) == normalize_os_path(path);
        s.unc = unc_for(&s.name, &s.local_path, path);
        out.push(s);
    }
    Ok(out)
}

#[cfg(not(windows))]
fn shares_for_os_path(_path: &Path) -> Result<Vec<FolderShare>> {
    Ok(Vec::new())
}

#[cfg(windows)]
fn add_folder_share_os(path: &Path, name: &str, remark: &str) -> Result<FolderShare> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Storage::FileSystem::{
        NetShareAdd, SHARE_INFO_2, SHARE_INFO_PERMISSIONS, SHI_USES_UNLIMITED, STYPE_DISKTREE,
    };

    if name.is_empty()
        || is_admin_share_name(name)
        || name.contains(['\\', '/', ':', '*', '?', '"', '<', '>', '|'])
    {
        return Err(FsError::InvalidPath {
            reason: "invalid share name".into(),
        });
    }
    let meta = std::fs::metadata(path)?;
    if !meta.is_dir() {
        return Err(FsError::Io(std::io::Error::other(
            "fm-error-share-not-folder",
        )));
    }
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut name_w = wide(name);
    let mut path_w = wide(&abs);
    let mut remark_w = wide(remark);
    let info = SHARE_INFO_2 {
        shi2_netname: PWSTR(name_w.as_mut_ptr()),
        shi2_type: STYPE_DISKTREE,
        shi2_remark: PWSTR(remark_w.as_mut_ptr()),
        shi2_permissions: SHARE_INFO_PERMISSIONS(0),
        shi2_max_uses: SHI_USES_UNLIMITED,
        shi2_current_uses: 0,
        shi2_path: PWSTR(path_w.as_mut_ptr()),
        shi2_passwd: PWSTR::null(),
    };
    let status = unsafe { NetShareAdd(PCWSTR::null(), 2, std::ptr::from_ref(&info).cast(), None) };
    if status != 0 {
        return Err(map_share_status(status));
    }
    Ok(FolderShare {
        name: name.to_string(),
        local_path: abs.clone(),
        remark: remark.to_string(),
        unc: unc_for(name, &abs, &abs),
        max_uses: None,
        current_uses: 0,
        administrative: false,
        exact: true,
    })
}

#[cfg(not(windows))]
fn add_folder_share_os(_path: &Path, _name: &str, _remark: &str) -> Result<FolderShare> {
    Err(FsError::Io(std::io::Error::other(
        "fm-error-share-unsupported",
    )))
}

#[cfg(windows)]
fn remove_folder_share_os(name: &str) -> Result<()> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::NetShareDel;

    if name.is_empty() {
        return Err(FsError::NotFound("share".into()));
    }
    if is_admin_share_name(name) {
        return Err(FsError::Io(std::io::Error::other("fm-error-share-admin")));
    }
    let mut name_w = wide(name);
    let status = unsafe { NetShareDel(PCWSTR::null(), PCWSTR(name_w.as_mut_ptr()), None) };
    if status != 0 {
        return Err(map_share_status(status));
    }
    Ok(())
}

#[cfg(not(windows))]
fn remove_folder_share_os(_name: &str) -> Result<()> {
    Err(FsError::Io(std::io::Error::other(
        "fm-error-share-unsupported",
    )))
}

#[cfg(windows)]
fn open_sharing_tab_os(path: &Path) -> Result<()> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::{SHObjectProperties, SHOP_FILEPATH};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let wide = wide(&abs);
    let hwnd = unsafe { GetForegroundWindow() };
    let ok = unsafe {
        SHObjectProperties(
            Some(hwnd),
            SHOP_FILEPATH,
            PCWSTR(wide.as_ptr()),
            w!("Sharing"),
        )
    };
    if ok.as_bool() {
        return Ok(());
    }
    let ok = unsafe {
        SHObjectProperties(
            Some(hwnd),
            SHOP_FILEPATH,
            PCWSTR(wide.as_ptr()),
            PCWSTR::null(),
        )
    };
    if ok.as_bool() {
        Ok(())
    } else {
        Err(FsError::Io(std::io::Error::other("fm-error-share-os")))
    }
}

#[cfg(not(windows))]
fn open_sharing_tab_os(_path: &Path) -> Result<()> {
    Err(FsError::Io(std::io::Error::other(
        "fm-error-share-unsupported",
    )))
}

#[cfg(windows)]
fn list_disk_shares() -> Result<Vec<FolderShare>> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        NetShareEnum, SHARE_INFO_2, SHI_USES_UNLIMITED, STYPE_DISKTREE, STYPE_SPECIAL,
    };

    let mut buf: *mut u8 = std::ptr::null_mut();
    let mut read = 0u32;
    let mut total = 0u32;
    let status = unsafe {
        NetShareEnum(
            PCWSTR::null(),
            2,
            &mut buf,
            u32::MAX,
            &mut read,
            &mut total,
            None,
        )
    };
    if status != 0 && status != 234 {
        return Err(map_share_status(status));
    }
    let mut out = Vec::new();
    if !buf.is_null() && read > 0 {
        let rows = unsafe { std::slice::from_raw_parts(buf.cast::<SHARE_INFO_2>(), read as usize) };
        for row in rows {
            let kind = row.shi2_type.0 & 0xF;
            if kind != STYPE_DISKTREE.0 {
                continue;
            }
            let name = pwstr_to_string(row.shi2_netname);
            if name.is_empty() {
                continue;
            }
            let local = PathBuf::from(pwstr_to_string(row.shi2_path));
            if local.as_os_str().is_empty() {
                continue;
            }
            let administrative =
                is_admin_share_name(&name) || (row.shi2_type.0 & STYPE_SPECIAL.0) != 0;
            out.push(FolderShare {
                name: name.clone(),
                local_path: local.clone(),
                remark: pwstr_to_string(row.shi2_remark),
                unc: unc_for(&name, &local, &local),
                max_uses: (row.shi2_max_uses != SHI_USES_UNLIMITED).then_some(row.shi2_max_uses),
                current_uses: row.shi2_current_uses,
                administrative,
                exact: true,
            });
        }
    }
    if !buf.is_null() {
        unsafe {
            net_api_buffer_free(buf.cast());
        }
    }
    Ok(out)
}

#[cfg(windows)]
fn wide(value: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    value
        .as_ref()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe { p.to_string().unwrap_or_default() }
}

#[cfg(windows)]
unsafe fn net_api_buffer_free(buffer: *mut std::ffi::c_void) -> u32 {
    #[link(name = "netapi32")]
    extern "system" {
        fn NetApiBufferFree(buffer: *mut std::ffi::c_void) -> u32;
    }
    unsafe { NetApiBufferFree(buffer) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_share_names() {
        assert!(is_admin_share_name("C$"));
        assert!(is_admin_share_name("admin$"));
        assert!(is_admin_share_name("IPC$"));
        assert!(!is_admin_share_name("Documents"));
        assert!(!is_admin_share_name("Photos$"));
    }

    #[test]
    fn share_root_covers_nested_path() {
        let root = Path::new(r"c:\data");
        assert!(share_covers_path(root, Path::new(r"c:\data")));
        assert!(share_covers_path(root, Path::new(r"c:\data\photos\a.jpg")));
        assert!(!share_covers_path(root, Path::new(r"c:\other")));
        assert!(!share_covers_path(root, Path::new(r"c:\data2")));
    }
}
