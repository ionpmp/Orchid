//! Bulk attributes, timestamps, name case, chmod / chown, and Windows ACL.

use std::path::Path;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use filetime::FileTime;

use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// Optional attribute toggles. `None` leaves the bit unchanged.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttrPatch {
    /// Read-only / write-protect.
    pub readonly: Option<bool>,
    /// Hidden (Windows; ignored on Unix).
    pub hidden: Option<bool>,
    /// System (Windows; ignored on Unix).
    pub system: Option<bool>,
    /// Archive (Windows; ignored on Unix).
    pub archive: Option<bool>,
}

/// How to rewrite a file name's case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameCase {
    /// `file name.txt`
    Lower,
    /// `FILE NAME.TXT`
    Upper,
    /// `File Name.Txt`
    Title,
}

/// Apply [`AttrPatch`] to a local path.
///
/// # Errors
///
/// Non-local paths or OS failures.
pub async fn apply_attr_patch(path: &FsPath, patch: AttrPatch) -> Result<()> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || patch_attrs_blocking(&os, patch))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn patch_attrs_blocking(path: &Path, patch: AttrPatch) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::{
            GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN,
            FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM, INVALID_FILE_ATTRIBUTES,
        };
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let raw = unsafe { GetFileAttributesW(PCWSTR(wide.as_ptr())) };
        if raw == INVALID_FILE_ATTRIBUTES {
            return Err(FsError::Io(std::io::Error::last_os_error()));
        }
        let mut attrs = raw;
        let mut apply = |on: Option<bool>, flag: u32| {
            if let Some(v) = on {
                if v {
                    attrs |= flag;
                } else {
                    attrs &= !flag;
                }
            }
        };
        apply(patch.readonly, FILE_ATTRIBUTE_READONLY.0);
        apply(patch.hidden, FILE_ATTRIBUTE_HIDDEN.0);
        apply(patch.system, FILE_ATTRIBUTE_SYSTEM.0);
        apply(patch.archive, FILE_ATTRIBUTE_ARCHIVE.0);
        unsafe {
            SetFileAttributesW(
                PCWSTR(wide.as_ptr()),
                windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(attrs),
            )
            .map_err(|e| FsError::Io(std::io::Error::other(e)))?;
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (patch.hidden, patch.system, patch.archive);
        if let Some(ro) = patch.readonly {
            let meta = std::fs::metadata(path)?;
            let mut perms = meta.permissions();
            perms.set_readonly(ro);
            std::fs::set_permissions(path, perms)?;
        }
        Ok(())
    }
}

/// Set modified (and optionally accessed) time to `when`, or now if `None`.
///
/// # Errors
///
/// Non-local paths or OS failures.
pub async fn set_mtime(path: &FsPath, when: Option<DateTime<Utc>>) -> Result<()> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || {
        let ft = match when {
            Some(dt) => {
                let st: SystemTime = SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_secs(dt.timestamp().max(0) as u64);
                FileTime::from_system_time(st)
            }
            None => FileTime::now(),
        };
        filetime::set_file_mtime(&os, ft).map_err(FsError::Io)
    })
    .await
    .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

/// Parse a datetime for [`set_mtime`]: RFC3339 or `YYYY-MM-DD HH:MM[:SS]`.
#[must_use]
pub fn parse_timestamp(s: &str) -> Option<DateTime<Utc>> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(t) {
        return Some(dt.with_timezone(&Utc));
    }
    chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M"))
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d")
                .map(|d| d.and_hms_opt(0, 0, 0).unwrap_or_default())
        })
        .ok()
        .map(|n| n.and_utc())
}

/// Rename `path` in place using [`NameCase`]. Returns the new path (or the old
/// one when the name is unchanged).
///
/// # Errors
///
/// Provider rename failures.
pub async fn apply_name_case(
    registry: &FsProviderRegistry,
    path: &FsPath,
    mode: NameCase,
) -> Result<FsPath> {
    let Some(name) = path.file_name() else {
        return Ok(path.clone());
    };
    let new_name = match mode {
        NameCase::Lower => name.to_lowercase(),
        NameCase::Upper => name.to_uppercase(),
        NameCase::Title => title_case(name),
    };
    if new_name == name {
        return Ok(path.clone());
    }
    let Some(parent) = path.parent() else {
        return Ok(path.clone());
    };
    let dest = parent.join(&new_name);
    let provider = registry
        .for_path(path)
        .ok_or_else(|| FsError::ProviderNotMounted(path.to_string()))?;
    provider.rename(path, &dest).await?;
    Ok(dest)
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut cap = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if cap {
                out.extend(c.to_uppercase());
                cap = false;
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            out.push(c);
            cap = true;
        }
    }
    out
}

/// POSIX mode bits (`0o755`). On Windows only the owner-write bit maps to
/// read-only.
///
/// # Errors
///
/// OS permission errors.
pub async fn chmod(path: &FsPath, mode: u32) -> Result<()> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || chmod_blocking(&os, mode))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn chmod_blocking(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let write = (mode & 0o222) != 0;
        let meta = std::fs::metadata(path)?;
        let mut perms = meta.permissions();
        perms.set_readonly(!write);
        std::fs::set_permissions(path, perms)?;
        Ok(())
    }
}

/// Parse an octal mode string (`"755"` / `"0755"`).
#[must_use]
pub fn parse_mode(s: &str) -> Option<u32> {
    let t = s.trim().trim_start_matches("0o").trim_start_matches('o');
    u32::from_str_radix(t, 8).ok().filter(|m| *m <= 0o7777)
}

/// Change owner / group on Unix. `user` and `group` may be names or numeric ids.
/// Either side of `user:group` may be empty.
///
/// # Errors
///
/// Unknown names, or OS `chown` failures. Always errors on Windows.
pub async fn chown(path: &FsPath, spec: &str) -> Result<()> {
    let os = path.to_local()?;
    let spec = spec.to_string();
    tokio::task::spawn_blocking(move || chown_blocking(&os, &spec))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn chown_blocking(path: &Path, spec: &str) -> Result<()> {
    #[cfg(unix)]
    {
        let (user, group) = spec.split_once(':').unwrap_or((spec, ""));
        let uid = if user.trim().is_empty() {
            None
        } else {
            Some(lookup_uid(user.trim())?)
        };
        let gid = if group.trim().is_empty() {
            None
        } else {
            Some(lookup_gid(group.trim())?)
        };
        std::os::unix::fs::chown(path, uid, gid).map_err(FsError::Io)
    }
    #[cfg(not(unix))]
    {
        let _ = (path, spec);
        Err(FsError::InvalidPath {
            reason: "changing owner/group is only supported on Unix".into(),
        })
    }
}

#[cfg(unix)]
fn lookup_uid(name: &str) -> Result<u32> {
    if let Ok(n) = name.parse::<u32>() {
        return Ok(n);
    }
    let c = std::ffi::CString::new(name).map_err(|_| FsError::InvalidPath {
        reason: "invalid user name".into(),
    })?;
    unsafe {
        let pw = libc::getpwnam(c.as_ptr());
        if pw.is_null() {
            Err(FsError::InvalidPath {
                reason: format!("unknown user `{name}`"),
            })
        } else {
            Ok((*pw).pw_uid)
        }
    }
}

#[cfg(unix)]
fn lookup_gid(name: &str) -> Result<u32> {
    if let Ok(n) = name.parse::<u32>() {
        return Ok(n);
    }
    let c = std::ffi::CString::new(name).map_err(|_| FsError::InvalidPath {
        reason: "invalid group name".into(),
    })?;
    unsafe {
        let gr = libc::getgrnam(c.as_ptr());
        if gr.is_null() {
            Err(FsError::InvalidPath {
                reason: format!("unknown group `{name}`"),
            })
        } else {
            Ok((*gr).gr_gid)
        }
    }
}

/// Read a textual ACL listing (`icacls` on Windows, `ls -ld` / `getfacl` on Unix).
///
/// # Errors
///
/// Non-local paths or process failures.
pub async fn acl_text(path: &FsPath) -> Result<String> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || acl_text_blocking(&os))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn acl_text_blocking(path: &Path) -> Result<String> {
    #[cfg(windows)]
    {
        run_hidden("icacls", &[path.as_os_str()])
    }
    #[cfg(unix)]
    {
        if let Ok(out) = std::process::Command::new("getfacl")
            .arg("-p")
            .arg(path)
            .output()
        {
            if out.status.success() {
                return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
            }
        }
        let meta = std::fs::metadata(path)?;
        use std::os::unix::fs::PermissionsExt;
        Ok(format!(
            "{} mode {:o}",
            path.display(),
            meta.permissions().mode() & 0o7777
        ))
    }
}

/// Grant `rights` (`F` full / `M` modify / `R` read) to `account` (Windows).
/// On Unix this is a no-op error directing the user to chmod/chown.
///
/// # Errors
///
/// `icacls` failures, or unsupported platform.
pub async fn acl_grant(path: &FsPath, account: &str, rights: &str) -> Result<String> {
    let os = path.to_local()?;
    let account = account.to_string();
    let rights = rights.to_string();
    tokio::task::spawn_blocking(move || acl_grant_blocking(&os, &account, &rights))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn acl_grant_blocking(path: &Path, account: &str, rights: &str) -> Result<String> {
    #[cfg(windows)]
    {
        let grant = format!("{account}:({rights})");
        run_hidden(
            "icacls",
            &[
                path.as_os_str(),
                std::ffi::OsStr::new("/grant"),
                std::ffi::OsStr::new(&grant),
            ],
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (path, account, rights);
        Err(FsError::InvalidPath {
            reason: "Windows ACL grant is not available on this OS".into(),
        })
    }
}

/// Reset ACL to inherited defaults (`icacls /reset`).
///
/// # Errors
///
/// Process failures, or unsupported platform.
pub async fn acl_reset(path: &FsPath) -> Result<String> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || acl_reset_blocking(&os))
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?
}

fn acl_reset_blocking(path: &Path) -> Result<String> {
    #[cfg(windows)]
    {
        run_hidden(
            "icacls",
            &[path.as_os_str(), std::ffi::OsStr::new("/reset")],
        )
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(FsError::InvalidPath {
            reason: "Windows ACL reset is not available on this OS".into(),
        })
    }
}

#[cfg(windows)]
fn run_hidden(cmd: &str, args: &[&std::ffi::OsStr]) -> Result<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let out = std::process::Command::new(cmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if out.status.success() {
        Ok(format!("{stdout}{stderr}"))
    } else {
        Err(FsError::Io(std::io::Error::other(format!(
            "{cmd} failed: {stderr}{stdout}"
        ))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_and_mode_parse() {
        assert_eq!(title_case("hello-world.txt"), "Hello-World.Txt");
        assert_eq!(parse_mode("755"), Some(0o755));
        assert_eq!(parse_mode("0755"), Some(0o755));
        assert!(parse_mode("zzz").is_none());
        assert!(parse_timestamp("2020-01-02 03:04").is_some());
    }

    #[tokio::test]
    async fn case_rename_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("Hello.TXT");
        std::fs::write(&p, b"x").unwrap();
        let reg = crate::provider::FsProviderRegistry::new();
        reg.register(std::sync::Arc::new(crate::provider::LocalProvider::new()))
            .unwrap();
        let fp = FsPath::from_local(&p).unwrap();
        let dest = apply_name_case(&reg, &fp, NameCase::Lower).await.unwrap();
        assert_eq!(dest.file_name(), Some("hello.txt"));
    }
}
