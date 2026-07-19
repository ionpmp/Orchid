//! Startup apps: registry Run keys + Startup folders.

use crate::widget::payloads::StartupRowView;

#[cfg(windows)]
#[allow(missing_docs)]
mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use windows::core::{w, GUID, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_SZ,
    };
    use windows::Win32::UI::Shell::{
        SHGetKnownFolderPath, FOLDERID_CommonStartup, FOLDERID_Startup, KF_FLAG_DEFAULT,
    };

    use super::StartupRowView;

    pub fn list_startup() -> Result<Vec<StartupRowView>, String> {
        let mut out = Vec::new();
        out.extend(enum_run_key(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            "HKCU\\Run",
            "hkcu",
        ));
        out.extend(enum_run_key(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\Windows\CurrentVersion\RunOnce",
            "HKCU\\RunOnce",
            "hkcu-once",
        ));
        out.extend(enum_run_key(
            HKEY_LOCAL_MACHINE,
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            "HKLM\\Run",
            "hklm",
        ));
        out.extend(enum_run_key(
            HKEY_LOCAL_MACHINE,
            r"Software\Microsoft\Windows\CurrentVersion\RunOnce",
            "HKLM\\RunOnce",
            "hklm-once",
        ));
        out.extend(enum_startup_folder(
            &FOLDERID_Startup,
            "Startup folder",
            "user-folder",
        ));
        out.extend(enum_startup_folder(
            &FOLDERID_CommonStartup,
            "Common Startup",
            "common-folder",
        ));
        out.sort_by(|a, b| a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()));
        Ok(out)
    }

    pub fn set_startup_enabled(id: &str, enabled: bool) -> Result<(), String> {
        let Some(rest) = id.strip_prefix("registry:") else {
            return Err("folder startup entries cannot be toggled yet".into());
        };
        let mut parts = rest.splitn(3, ':');
        let hive = parts.next().unwrap_or("");
        let _sub = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if name.is_empty() {
            return Err("invalid startup id".into());
        }
        let (root, approved_path) = match hive {
            "hkcu" | "hkcu-once" => (
                HKEY_CURRENT_USER,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"),
            ),
            "hklm" | "hklm-once" => (
                HKEY_LOCAL_MACHINE,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"),
            ),
            _ => return Err("unknown registry hive".into()),
        };
        let mut data = [0u8; 12];
        data[0] = if enabled { 0x02 } else { 0x03 };
        let name_wide: Vec<u16> = OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let mut key = HKEY::default();
            RegOpenKeyExW(root, approved_path, Some(0), KEY_SET_VALUE, &mut key)
                .ok()
                .map_err(|e| format!("open StartupApproved: {e}"))?;
            let status =
                RegSetValueExW(key, PCWSTR(name_wide.as_ptr()), Some(0), REG_BINARY, Some(&data));
            let _ = RegCloseKey(key);
            status
                .ok()
                .map_err(|e| format!("set StartupApproved: {e}"))?;
        }
        Ok(())
    }

    pub fn open_startup_location(id: &str) -> Result<(), String> {
        if let Some(path) = id.strip_prefix("folder:") {
            return opener::open(Path::new(path).parent().unwrap_or(Path::new(path)))
                .map_err(|e| e.to_string());
        }
        if id.starts_with("registry:hkcu") {
            return opener::open("shell:startup").map_err(|e| e.to_string());
        }
        if id.starts_with("registry:hklm") {
            return opener::open(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\StartUp")
                .map_err(|e| e.to_string());
        }
        Err("unknown startup location".into())
    }

    fn enum_run_key(root: HKEY, path: &str, location: &str, hive: &str) -> Vec<StartupRowView> {
        let path_wide: Vec<u16> = OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut out = Vec::new();
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(root, PCWSTR(path_wide.as_ptr()), Some(0), KEY_READ, &mut key)
                .is_err()
            {
                return out;
            }
            let approved = read_approved_map(root);
            let mut index = 0u32;
            loop {
                let mut name_buf = [0u16; 256];
                let mut name_len = name_buf.len() as u32;
                let mut data_buf = [0u8; 4096];
                let mut data_len = data_buf.len() as u32;
                let mut ty = 0u32;
                let status = RegEnumValueW(
                    key,
                    index,
                    Some(PWSTR(name_buf.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut ty),
                    Some(data_buf.as_mut_ptr()),
                    Some(&mut data_len),
                );
                if status == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if status != ERROR_SUCCESS {
                    break;
                }
                index += 1;
                let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let command = if ty == REG_SZ.0 {
                    let u16s = std::slice::from_raw_parts(
                        data_buf.as_ptr().cast::<u16>(),
                        (data_len as usize / 2).saturating_sub(1),
                    );
                    String::from_utf16_lossy(u16s)
                } else {
                    String::new()
                };
                let enabled = approved.get(&name).copied().unwrap_or(true);
                out.push(StartupRowView {
                    id: format!("registry:{hive}:{path}:{name}"),
                    name,
                    command,
                    location: location.into(),
                    enabled,
                    can_toggle: true,
                });
            }
            let _ = RegCloseKey(key);
        }
        out
    }

    fn read_approved_map(root: HKEY) -> std::collections::HashMap<String, bool> {
        let mut map = std::collections::HashMap::new();
        let path = w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(root, path, Some(0), KEY_READ, &mut key).is_err() {
                return map;
            }
            let mut index = 0u32;
            loop {
                let mut name_buf = [0u16; 256];
                let mut name_len = name_buf.len() as u32;
                let mut data_buf = [0u8; 64];
                let mut data_len = data_buf.len() as u32;
                let mut ty = 0u32;
                let status = RegEnumValueW(
                    key,
                    index,
                    Some(PWSTR(name_buf.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut ty),
                    Some(data_buf.as_mut_ptr()),
                    Some(&mut data_len),
                );
                if status == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if status != ERROR_SUCCESS {
                    break;
                }
                index += 1;
                let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
                let enabled = data_buf
                    .first()
                    .map(|b| *b == 0x02 || *b == 0x00)
                    .unwrap_or(true);
                map.insert(name, enabled);
            }
            let _ = RegCloseKey(key);
        }
        map
    }

    fn enum_startup_folder(folder_id: &GUID, location: &str, tag: &str) -> Vec<StartupRowView> {
        let Some(dir) = known_folder(folder_id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let command = path.to_string_lossy().into_owned();
            out.push(StartupRowView {
                id: format!("folder:{command}"),
                name,
                command,
                location: format!("{location} ({tag})"),
                enabled: true,
                can_toggle: false,
            });
        }
        out
    }

    fn known_folder(id: &GUID) -> Option<PathBuf> {
        unsafe {
            let pwstr = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
            if pwstr.is_null() {
                return None;
            }
            let s = pwstr.to_string().ok()?;
            CoTaskMemFree(Some(pwstr.0 as *const _));
            Some(PathBuf::from(s))
        }
    }
}

#[cfg(windows)]
pub use win::{list_startup, open_startup_location, set_startup_enabled};

#[cfg(not(windows))]
pub fn list_startup() -> Result<Vec<StartupRowView>, String> {
    Ok(Vec::new())
}

#[cfg(not(windows))]
pub fn set_startup_enabled(_id: &str, _enabled: bool) -> Result<(), String> {
    Err("startup apps are only supported on Windows".into())
}

#[cfg(not(windows))]
pub fn open_startup_location(_id: &str) -> Result<(), String> {
    Err("startup apps are only supported on Windows".into())
}
