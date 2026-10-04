//! Startup apps: registry Run keys + Startup folders.

use crate::widget::payloads::StartupRowView;

#[cfg(windows)]
#[allow(missing_docs)]
mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use windows::core::{w, GUID, PCWSTR, PWSTR};
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS};
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegEnumValueW, RegOpenKeyExW, RegSetKeyValueW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_SZ,
    };
    use windows::Win32::UI::Shell::{
        FOLDERID_CommonStartup, FOLDERID_Startup, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
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
            "processes-startup-user-folder",
            HKEY_CURRENT_USER,
        ));
        out.extend(enum_startup_folder(
            &FOLDERID_CommonStartup,
            "processes-startup-common-folder",
            HKEY_LOCAL_MACHINE,
        ));
        out.sort_by(|a, b| {
            a.name
                .to_ascii_lowercase()
                .cmp(&b.name.to_ascii_lowercase())
        });
        Ok(out)
    }

    pub fn set_startup_enabled(id: &str, enabled: bool) -> Result<(), String> {
        if let Some(path) = id.strip_prefix("folder:") {
            return set_folder_enabled(Path::new(path), enabled);
        }
        let Some(rest) = id.strip_prefix("registry:") else {
            return Err("unknown startup id".into());
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
        let direct = unsafe {
            let mut key = HKEY::default();
            if let Err(error) =
                RegOpenKeyExW(root, approved_path, Some(0), KEY_SET_VALUE, &mut key).ok()
            {
                Err(error)
            } else {
                let status = RegSetValueExW(
                    key,
                    PCWSTR(name_wide.as_ptr()),
                    Some(0),
                    REG_BINARY,
                    Some(&data),
                );
                let _ = RegCloseKey(key);
                status.ok()
            }
        };
        if let Err(error) = direct {
            if matches!(hive, "hklm" | "hklm-once")
                && error.code() == ERROR_ACCESS_DENIED.to_hresult()
            {
                return elevate_binary(
                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
                    name,
                    &data,
                );
            }
            return Err(format!("set StartupApproved: {error}"));
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
            if RegOpenKeyExW(
                root,
                PCWSTR(path_wide.as_ptr()),
                Some(0),
                KEY_READ,
                &mut key,
            )
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
        read_approved_at(
            root,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"),
        )
    }

    fn read_approved_at(root: HKEY, path: PCWSTR) -> std::collections::HashMap<String, bool> {
        let mut map = std::collections::HashMap::new();
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

    fn enum_startup_folder(folder_id: &GUID, location: &str, root: HKEY) -> Vec<StartupRowView> {
        let Some(dir) = known_folder(folder_id) else {
            return Vec::new();
        };
        let approved = read_approved_at(
            root,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"),
        );
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
            let enabled = approved.get(&name).copied().unwrap_or(true);
            out.push(StartupRowView {
                id: format!("folder:{command}"),
                name,
                command,
                location: location.into(),
                enabled,
                can_toggle: true,
            });
        }
        out
    }

    fn set_folder_enabled(path: &Path, enabled: bool) -> Result<(), String> {
        let name = path
            .file_name()
            .ok_or_else(|| "startup shortcut has no name".to_string())?;
        let user = known_folder(&FOLDERID_Startup);
        let common = known_folder(&FOLDERID_CommonStartup);
        let root = if user.as_ref().is_some_and(|dir| path.starts_with(dir)) {
            HKEY_CURRENT_USER
        } else if common.as_ref().is_some_and(|dir| path.starts_with(dir)) {
            HKEY_LOCAL_MACHINE
        } else {
            return Err("startup shortcut is outside the Startup folders".into());
        };
        let name = name.to_string_lossy();
        let machine = root == HKEY_LOCAL_MACHINE;
        write_approved(
            root,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"),
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder",
            &name,
            enabled,
            machine,
        )
    }

    fn write_approved(
        root: HKEY,
        subkey: PCWSTR,
        subkey_text: &str,
        name: &str,
        enabled: bool,
        machine: bool,
    ) -> Result<(), String> {
        let mut data = [0u8; 12];
        data[0] = if enabled { 0x02 } else { 0x03 };
        let name_wide: Vec<u16> = OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let status = unsafe {
            RegSetKeyValueW(
                root,
                subkey,
                PCWSTR(name_wide.as_ptr()),
                REG_BINARY.0,
                Some(data.as_ptr().cast()),
                data.len() as u32,
            )
        };
        match status.ok() {
            Ok(()) => Ok(()),
            Err(error) if machine && error.code() == ERROR_ACCESS_DENIED.to_hresult() => {
                elevate_binary(subkey_text, name, &data)
            }
            Err(error) => Err(format!("set StartupApproved: {error}")),
        }
    }

    fn elevate_binary(subkey: &str, name: &str, data: &[u8]) -> Result<(), String> {
        let hex = data
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(",");
        let escaped = name.replace('\\', "\\\\").replace('"', "\\\"");
        let text = format!(
            "Windows Registry Editor Version 5.00\r\n\r\n[HKEY_LOCAL_MACHINE\\{subkey}]\r\n\"{escaped}\"=hex:{hex}\r\n"
        );
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        let path =
            std::env::temp_dir().join(format!("orchid-startup-{}.reg", uuid::Uuid::new_v4()));
        std::fs::write(&path, bytes).map_err(|error| error.to_string())?;
        let shown = path.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$p = Start-Process -FilePath reg.exe -ArgumentList @('import','{shown}') -Verb RunAs -Wait -PassThru -WindowStyle Hidden; if ($null -eq $p) {{ exit 1 }}; exit $p.ExitCode"
        );
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .output();
        let _ = std::fs::remove_file(&path);
        match output {
            Ok(output) if output.status.success() => Ok(()),
            Ok(_) => Err("access denied".into()),
            Err(error) => Err(error.to_string()),
        }
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
