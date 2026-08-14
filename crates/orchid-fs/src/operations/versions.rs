//! Previous Versions (Volume Shadow Copy) for a local path.

use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};

use crate::error::{FsError, Result};
use crate::path::FsPath;

/// One shadow-copy instance of a file or folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviousVersion {
    /// Snapshot creation time (UTC).
    pub created: DateTime<Utc>,
    /// Path inside the snapshot (`\\?\GLOBALROOT\…` or `\\localhost\X$\@GMT-…`).
    pub shadow_path: PathBuf,
    /// Size in bytes; `0` for directories.
    pub size: u64,
    /// Directory snapshot (recursive restore).
    pub is_dir: bool,
}

/// Shadow copies that still contain `path`, newest first.
///
/// # Errors
///
/// Non-local paths, access denied, or an unsupported platform.
pub async fn previous_versions(path: &FsPath) -> Result<Vec<PreviousVersion>> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || previous_versions_os(&os))
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// Overwrite `path` with the snapshot selected by `spec` (1-based index or timestamp).
///
/// # Errors
///
/// Missing versions, bad index, or I/O while copying.
pub async fn restore_previous_version(path: &FsPath, spec: &str) -> Result<PreviousVersion> {
    let os = path.to_local()?;
    let spec = spec.to_string();
    tokio::task::spawn_blocking(move || restore_previous_version_os(&os, &spec))
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// Copy the selected snapshot beside `path` as `name (YYYY-MM-DD HH-MM-SS)`.
///
/// # Errors
///
/// Missing versions, name collision, or I/O while copying.
pub async fn copy_previous_version(
    path: &FsPath,
    spec: &str,
) -> Result<(PreviousVersion, PathBuf)> {
    let os = path.to_local()?;
    let spec = spec.to_string();
    tokio::task::spawn_blocking(move || copy_previous_version_os(&os, &spec))
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// Open the Windows Properties dialog on the Previous Versions page.
///
/// # Errors
///
/// Non-local paths or Shell failure.
pub async fn open_previous_versions_tab(path: &FsPath) -> Result<()> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || open_previous_versions_tab_os(&os))
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// Pick a snapshot by 1-based index (newest is `1`) or a timestamp prefix.
///
/// # Errors
///
/// Empty list or an unknown index / stamp.
pub fn pick_previous_version<'a>(
    versions: &'a [PreviousVersion],
    spec: &str,
) -> Result<&'a PreviousVersion> {
    if versions.is_empty() {
        return Err(FsError::Io(io::Error::other("fm-error-versions-none")));
    }
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok(&versions[0]);
    }
    if let Ok(n) = spec.parse::<usize>() {
        return versions
            .get(n.checked_sub(1).unwrap_or(usize::MAX))
            .ok_or_else(|| FsError::Io(io::Error::other("fm-error-versions-bad-index")));
    }
    let needle = spec.replace('T', " ");
    versions
        .iter()
        .find(|v| {
            let utc = v.created.format("%Y-%m-%d %H:%M:%S").to_string();
            let local = v
                .created
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string();
            utc.starts_with(&needle) || local.starts_with(&needle)
        })
        .ok_or_else(|| FsError::Io(io::Error::other("fm-error-versions-bad-index")))
}

/// File-name stamp (`2024-03-15 12-00-00`) in local time.
#[must_use]
pub fn previous_version_file_stamp(created: DateTime<Utc>) -> String {
    created
        .with_timezone(&Local)
        .format("%Y-%m-%d %H-%M-%S")
        .to_string()
}

fn restore_previous_version_os(path: &Path, spec: &str) -> Result<PreviousVersion> {
    let versions = previous_versions_os(path)?;
    let chosen = pick_previous_version(&versions, spec)?.clone();
    copy_snapshot(&chosen.shadow_path, path, chosen.is_dir)?;
    Ok(chosen)
}

fn copy_previous_version_os(path: &Path, spec: &str) -> Result<(PreviousVersion, PathBuf)> {
    let versions = previous_versions_os(path)?;
    let chosen = pick_previous_version(&versions, spec)?.clone();
    let dest = copy_beside_path(path, chosen.created, chosen.is_dir);
    if dest.exists() {
        return Err(FsError::Io(io::Error::other("fm-error-versions-collision")));
    }
    copy_snapshot(&chosen.shadow_path, &dest, chosen.is_dir)?;
    Ok((chosen, dest))
}

fn copy_beside_path(path: &Path, created: DateTime<Utc>, is_dir: bool) -> PathBuf {
    let stamp = previous_version_file_stamp(created);
    let parent = path.parent().unwrap_or(path);
    if is_dir {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "folder".into());
        return parent.join(format!("{name} ({stamp})"));
    }
    let stem = path
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".into());
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if !ext.is_empty() => parent.join(format!("{stem} ({stamp}).{ext}")),
        _ => parent.join(format!("{stem} ({stamp})")),
    }
}

fn copy_snapshot(from: &Path, to: &Path, is_dir: bool) -> Result<()> {
    if is_dir {
        copy_dir(from, to)
    } else {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)?;
        copy_mtime(from, to);
        Ok(())
    }
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    copy_mtime(from, to);
    for entry in walkdir::WalkDir::new(from).min_depth(1).follow_links(false) {
        let entry = entry.map_err(|e| FsError::Io(io::Error::other(e.to_string())))?;
        let rel = entry
            .path()
            .strip_prefix(from)
            .map_err(|e| FsError::Io(io::Error::other(e.to_string())))?;
        let dest = to.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_mtime(entry.path(), &dest);
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &dest)?;
            copy_mtime(entry.path(), &dest);
        }
    }
    Ok(())
}

fn copy_mtime(from: &Path, to: &Path) {
    if let Ok(meta) = std::fs::metadata(from) {
        if let Ok(mtime) = meta.modified() {
            let _ = filetime::set_file_mtime(to, filetime::FileTime::from_system_time(mtime));
        }
    }
}

#[cfg(windows)]
fn previous_versions_os(path: &Path) -> Result<Vec<PreviousVersion>> {
    let abs = normalize_abs(path);
    let (mount, volume, relative) = volume_parts(&abs)?;
    let wmi = list_wmi_shadows(&volume);
    let mut versions = match wmi {
        Ok(shadows) => versions_from_shadows(&shadows, &relative),
        Err(e) => {
            let gmt = list_gmt_versions(&mount, &relative);
            if !gmt.is_empty() {
                gmt
            } else {
                return Err(e);
            }
        }
    };
    if versions.is_empty() {
        let gmt = list_gmt_versions(&mount, &relative);
        if !gmt.is_empty() {
            versions = gmt;
        }
    }
    versions.sort_by_key(|b| std::cmp::Reverse(b.created));
    versions.dedup_by(|a, b| a.created == b.created && a.size == b.size && a.is_dir == b.is_dir);
    Ok(versions)
}

#[cfg(not(windows))]
fn previous_versions_os(_path: &Path) -> Result<Vec<PreviousVersion>> {
    Err(FsError::Io(io::Error::other(
        "fm-error-versions-unsupported",
    )))
}

#[cfg(windows)]
fn open_previous_versions_tab_os(path: &Path) -> Result<()> {
    use windows::core::{w, PCWSTR};
    use windows::Win32::UI::Shell::{SHObjectProperties, SHOP_FILEPATH};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let wide = wide(&abs);
    let hwnd = unsafe { GetForegroundWindow() };
    for page in [
        w!("Previous Versions"),
        w!("{596AB062-B4D2-4215-9F74-E9109B0A8153}"),
    ] {
        let ok =
            unsafe { SHObjectProperties(Some(hwnd), SHOP_FILEPATH, PCWSTR(wide.as_ptr()), page) };
        if ok.as_bool() {
            return Ok(());
        }
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
        Err(FsError::Io(io::Error::other("fm-error-versions-os")))
    }
}

#[cfg(not(windows))]
fn open_previous_versions_tab_os(_path: &Path) -> Result<()> {
    Err(FsError::Io(io::Error::other(
        "fm-error-versions-unsupported",
    )))
}

#[cfg(windows)]
struct ShadowDevice {
    device: String,
    created: DateTime<Utc>,
}

#[cfg(windows)]
fn versions_from_shadows(shadows: &[ShadowDevice], relative: &str) -> Vec<PreviousVersion> {
    let mut out = Vec::new();
    for snap in shadows {
        let shadow_path = join_shadow(&snap.device, relative);
        let Ok(meta) = std::fs::metadata(&shadow_path) else {
            continue;
        };
        out.push(PreviousVersion {
            created: snap.created,
            shadow_path,
            size: if meta.is_dir() { 0 } else { meta.len() },
            is_dir: meta.is_dir(),
        });
    }
    out
}

#[cfg(windows)]
fn list_wmi_shadows(volume: &str) -> Result<Vec<ShadowDevice>> {
    use windows::core::{w, BSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows::Win32::System::Wmi::{
        IEnumWbemClassObject, IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator,
        WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE,
    };

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                unsafe { CoUninitialize() };
            }
        }
    }

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let _com = ComGuard(initialized);

    let locator: IWbemLocator = unsafe {
        CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(map_versions_com)?
    };
    let empty = BSTR::new();
    let services: IWbemServices = unsafe {
        locator
            .ConnectServer(
                &BSTR::from("ROOT\\CIMV2"),
                &empty,
                &empty,
                &empty,
                0,
                &empty,
                None,
            )
            .map_err(map_versions_com)?
    };
    let _ = unsafe {
        CoSetProxyBlanket(
            &services,
            10,
            0,
            None,
            RPC_C_AUTHN_LEVEL_CALL,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            None,
            EOAC_NONE,
        )
    };
    let enumerator: IEnumWbemClassObject = unsafe {
        services
            .ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from("SELECT DeviceObject, InstallDate, VolumeName FROM Win32_ShadowCopy"),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
            .map_err(map_versions_com)?
    };

    let want = normalize_volume(volume);
    let mut out = Vec::new();
    loop {
        let mut slot: [Option<IWbemClassObject>; 1] = [None];
        let mut returned = 0u32;
        let hr = unsafe { enumerator.Next(WBEM_INFINITE, &mut slot, &mut returned) };
        if hr.is_err() || returned == 0 {
            break;
        }
        let Some(obj) = slot[0].take() else {
            continue;
        };
        let device = unsafe { wmi_string(&obj, w!("DeviceObject")) };
        let vol = unsafe { wmi_string(&obj, w!("VolumeName")) };
        let when = unsafe { wmi_string(&obj, w!("InstallDate")) };
        if device.is_empty() || !normalize_volume(&vol).eq_ignore_ascii_case(&want) {
            continue;
        }
        let Some(created) = parse_cim_datetime(&when) else {
            continue;
        };
        out.push(ShadowDevice { device, created });
    }
    Ok(out)
}

#[cfg(windows)]
unsafe fn wmi_string(
    obj: &windows::Win32::System::Wmi::IWbemClassObject,
    name: windows::core::PCWSTR,
) -> String {
    use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR};

    let mut var = VARIANT::default();
    let ok = unsafe { obj.Get(name, 0, &mut var, None, None) };
    let text = if ok.is_ok() {
        unsafe {
            let rec = &var.Anonymous.Anonymous;
            if rec.vt == VT_BSTR {
                rec.Anonymous.bstrVal.to_string()
            } else {
                String::new()
            }
        }
    } else {
        String::new()
    };
    let _ = unsafe { VariantClear(&mut var) };
    text
}

#[cfg(windows)]
fn list_gmt_versions(mount: &str, relative: &str) -> Vec<PreviousVersion> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        FindClose, FindFirstFileW, FindNextFileW, WIN32_FIND_DATAW,
    };

    let Some(letter) = drive_letter(mount) else {
        return Vec::new();
    };
    let pattern = wide(format!(r"\\localhost\{letter}$\@GMT-*"));
    let mut data = WIN32_FIND_DATAW::default();
    let handle = unsafe { FindFirstFileW(PCWSTR(pattern.as_ptr()), &mut data) };
    let Ok(handle) = handle else {
        return Vec::new();
    };
    let mut out = Vec::new();
    loop {
        let name = wchar_to_string(&data.cFileName);
        if let Some(created) = parse_gmt_name(&name) {
            let shadow_path = join_gmt(letter, &name, relative);
            if let Ok(meta) = std::fs::metadata(&shadow_path) {
                out.push(PreviousVersion {
                    created,
                    shadow_path,
                    size: if meta.is_dir() { 0 } else { meta.len() },
                    is_dir: meta.is_dir(),
                });
            }
        }
        if unsafe { FindNextFileW(handle, &mut data) }.is_err() {
            break;
        }
    }
    let _ = unsafe { FindClose(handle) };
    out
}

#[cfg(windows)]
fn volume_parts(path: &Path) -> Result<(String, String, String)> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        GetVolumeNameForVolumeMountPointW, GetVolumePathNameW,
    };

    let path_w = wide(path);
    let mut mount_buf = [0u16; 512];
    unsafe { GetVolumePathNameW(PCWSTR(path_w.as_ptr()), &mut mount_buf) }
        .map_err(|_| FsError::Io(io::Error::other("fm-error-versions-unsupported")))?;
    let mount = wchar_to_string(&mount_buf);
    if mount.is_empty() {
        return Err(FsError::Io(io::Error::other(
            "fm-error-versions-unsupported",
        )));
    }
    let mount_w = wide(&mount);
    let mut vol_buf = [0u16; 128];
    unsafe { GetVolumeNameForVolumeMountPointW(PCWSTR(mount_w.as_ptr()), &mut vol_buf) }
        .map_err(|_| FsError::Io(io::Error::other("fm-error-versions-unsupported")))?;
    let volume = wchar_to_string(&vol_buf);
    let relative = relative_to_mount(&path.to_string_lossy(), &mount);
    Ok((mount, volume, relative))
}

fn normalize_abs(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn relative_to_mount(abs: &str, mount: &str) -> String {
    let abs = strip_extended(abs).replace('/', "\\");
    let mount = strip_extended(mount).replace('/', "\\");
    let abs_l = abs.trim_end_matches('\\').to_ascii_lowercase();
    let mount_l = mount.trim_end_matches('\\').to_ascii_lowercase();
    if abs_l == mount_l {
        return String::new();
    }
    abs.trim_end_matches('\\')
        .get(mount.trim_end_matches('\\').len()..)
        .unwrap_or("")
        .trim_start_matches('\\')
        .to_string()
}

fn strip_extended(path: &str) -> &str {
    path.strip_prefix(r"\\?\")
        .or_else(|| path.strip_prefix(r"//?/"))
        .unwrap_or(path)
}

fn normalize_volume(volume: &str) -> String {
    let mut v = strip_extended(volume)
        .replace('/', "\\")
        .to_ascii_lowercase();
    if !v.ends_with('\\') {
        v.push('\\');
    }
    v
}

fn join_shadow(device: &str, relative: &str) -> PathBuf {
    let device = device.trim_end_matches(['\\', '/']);
    if relative.is_empty() {
        PathBuf::from(device)
    } else {
        PathBuf::from(format!("{device}\\{relative}"))
    }
}

fn join_gmt(letter: char, gmt_name: &str, relative: &str) -> PathBuf {
    let root = format!(r"\\localhost\{letter}$\{gmt_name}");
    if relative.is_empty() {
        PathBuf::from(root)
    } else {
        PathBuf::from(format!(r"{root}\{relative}"))
    }
}

fn drive_letter(mount: &str) -> Option<char> {
    let s = strip_extended(mount);
    let mut chars = s.chars();
    let letter = chars.next()?;
    if letter.is_ascii_alphabetic() && chars.next() == Some(':') {
        Some(letter.to_ascii_uppercase())
    } else {
        None
    }
}

fn parse_cim_datetime(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.len() < 14 {
        return None;
    }
    let year: i32 = raw.get(0..4)?.parse().ok()?;
    let month: u32 = raw.get(4..6)?.parse().ok()?;
    let day: u32 = raw.get(6..8)?.parse().ok()?;
    let hour: u32 = raw.get(8..10)?.parse().ok()?;
    let min: u32 = raw.get(10..12)?.parse().ok()?;
    let sec: u32 = raw.get(12..14)?.parse().ok()?;
    let naive = NaiveDate::from_ymd_opt(year, month, day)?
        .and_time(NaiveTime::from_hms_opt(hour, min, sec)?);
    if raw.len() >= 25 {
        let sign = raw.as_bytes().get(21).copied()?;
        let minutes: i32 = raw.get(22..25)?.parse().ok()?;
        let offset = if sign == b'-' { -minutes } else { minutes };
        return apply_cim_offset(naive, offset);
    }
    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

fn apply_cim_offset(naive: chrono::NaiveDateTime, offset_minutes: i32) -> Option<DateTime<Utc>> {
    use chrono::FixedOffset;
    let secs = offset_minutes.checked_mul(60)?;
    let tz = FixedOffset::east_opt(secs)?;
    tz.from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

fn parse_gmt_name(name: &str) -> Option<DateTime<Utc>> {
    let rest = name.strip_prefix("@GMT-")?;
    let (date, time) = rest.split_once('-')?;
    let mut d = date.split('.');
    let year: i32 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;
    let mut t = time.split('.');
    let hour: u32 = t.next()?.parse().ok()?;
    let min: u32 = t.next()?.parse().ok()?;
    let sec: u32 = t.next()?.parse().ok()?;
    let naive = NaiveDate::from_ymd_opt(year, month, day)?
        .and_time(NaiveTime::from_hms_opt(hour, min, sec)?);
    Some(DateTime::from_naive_utc_and_offset(naive, Utc))
}

#[cfg(windows)]
fn map_versions_com(err: windows::core::Error) -> FsError {
    let hr = err.code().0 as u32;
    match hr {
        0x8007_0005 | 0x8004_1003 | 0x8007_000E => {
            FsError::Io(io::Error::other("fm-error-versions-denied"))
        }
        _ => FsError::Io(io::Error::other("fm-error-versions-unsupported")),
    }
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
fn wchar_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cim_datetime_with_offset() {
        let dt = parse_cim_datetime("20240315120000.000000-000").unwrap();
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-03-15 12:00:00"
        );
        let east = parse_cim_datetime("20240315120000.000000+060").unwrap();
        assert_eq!(
            east.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-03-15 11:00:00"
        );
    }

    #[test]
    fn gmt_folder_name() {
        let dt = parse_gmt_name("@GMT-2024.03.15-12.00.00").unwrap();
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2024-03-15 12:00:00"
        );
        assert!(parse_gmt_name("not-a-snapshot").is_none());
    }

    #[test]
    fn relative_path_from_mount() {
        assert_eq!(
            relative_to_mount(r"C:\Users\foo\bar.txt", r"C:\"),
            r"Users\foo\bar.txt"
        );
        assert_eq!(relative_to_mount(r"C:\", r"C:\"), "");
        assert_eq!(relative_to_mount(r"\\?\C:\Users\foo", r"C:\"), r"Users\foo");
    }

    #[test]
    fn shadow_and_gmt_join() {
        assert_eq!(
            join_shadow(
                r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1",
                r"Users\a.txt"
            ),
            PathBuf::from(r"\\?\GLOBALROOT\Device\HarddiskVolumeShadowCopy1\Users\a.txt")
        );
        assert_eq!(
            join_gmt('C', "@GMT-2024.03.15-12.00.00", r"Users\a.txt"),
            PathBuf::from(r"\\localhost\C$\@GMT-2024.03.15-12.00.00\Users\a.txt")
        );
    }

    #[test]
    fn pick_by_index_and_stamp() {
        let v = vec![
            PreviousVersion {
                created: Utc.with_ymd_and_hms(2024, 3, 15, 12, 0, 0).unwrap(),
                shadow_path: PathBuf::from("a"),
                size: 1,
                is_dir: false,
            },
            PreviousVersion {
                created: Utc.with_ymd_and_hms(2024, 1, 1, 8, 0, 0).unwrap(),
                shadow_path: PathBuf::from("b"),
                size: 2,
                is_dir: false,
            },
        ];
        assert_eq!(pick_previous_version(&v, "1").unwrap().size, 1);
        assert_eq!(pick_previous_version(&v, "2").unwrap().size, 2);
        assert!(pick_previous_version(&v, "9").is_err());
        assert_eq!(
            pick_previous_version(&v, "2024-03-15 12:00").unwrap().size,
            1
        );
        assert!(pick_previous_version(&[], "1").is_err());
    }

    #[test]
    fn copy_beside_keeps_extension() {
        let dest = copy_beside_path(
            Path::new(r"C:\data\photo.jpg"),
            Utc.with_ymd_and_hms(2024, 3, 15, 12, 0, 0).unwrap(),
            false,
        );
        let name = dest.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("photo ("));
        assert!(name.ends_with(").jpg"));
    }
}
