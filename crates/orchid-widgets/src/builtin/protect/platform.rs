//! Operating-system actions that are not plain file deletes: recycle bin,
//! DNS cache, clipboard, registry MRUs, and the advertising identifier.
//!
//! Registry writes are limited to a fixed list of Explorer history keys.

/// A platform step failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlatformError {
    /// This operating system does not implement the step.
    Unavailable,
    /// The process needs to be elevated.
    NeedAdmin,
    /// A short description safe to show in the widget status line.
    Other(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "unavailable"),
            Self::NeedAdmin => write!(f, "need-admin"),
            Self::Other(text) => write!(f, "{text}"),
        }
    }
}

/// Empty the recycle bin without a confirmation dialog.
///
/// # Errors
///
/// Returns [`PlatformError::Unavailable`] off Windows, or [`PlatformError::NeedAdmin`]
/// when the shell refuses the call.
pub fn empty_recycle() -> Result<(), PlatformError> {
    #[cfg(windows)]
    {
        windows_impl::empty_recycle()
    }
    #[cfg(not(windows))]
    {
        Err(PlatformError::Unavailable)
    }
}

/// Flush the DNS resolver cache.
///
/// # Errors
///
/// Returns [`PlatformError::Unavailable`] off Windows.
pub fn flush_dns() -> Result<(), PlatformError> {
    #[cfg(windows)]
    {
        run_command("ipconfig", &["/flushdns"])
    }
    #[cfg(not(windows))]
    {
        Err(PlatformError::Unavailable)
    }
}

/// Clear the current clipboard contents.
///
/// # Errors
///
/// Returns an error when the clipboard cannot be opened.
pub fn clear_clipboard() -> Result<(), PlatformError> {
    let mut clipboard =
        arboard::Clipboard::new().map_err(|e| PlatformError::Other(e.to_string()))?;
    clipboard
        .clear()
        .map_err(|e| PlatformError::Other(e.to_string()))
}

/// Delete values under allowlisted Explorer history keys, including one level
/// of subkeys (RecentDocs extensions, UserAssist counts).
///
/// # Errors
///
/// Returns an error when `keys` contains a path outside the allowlist, or when
/// the registry cannot be opened.
pub fn clear_registry_keys(keys: &[&str]) -> Result<u32, PlatformError> {
    for key in keys {
        if !registry_path_allowed(key) {
            return Err(PlatformError::Other("registry path refused".into()));
        }
    }
    #[cfg(windows)]
    {
        windows_impl::clear_registry_keys(keys)
    }
    #[cfg(not(windows))]
    {
        let _ = keys;
        Err(PlatformError::Unavailable)
    }
}

/// Set the per-user advertising identifier to off.
///
/// # Errors
///
/// Returns [`PlatformError::Unavailable`] off Windows.
pub fn disable_advertising_id() -> Result<(), PlatformError> {
    #[cfg(windows)]
    {
        run_command(
            "reg",
            &[
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo",
                "/v",
                "Enabled",
                "/t",
                "REG_DWORD",
                "/d",
                "0",
                "/f",
            ],
        )
    }
    #[cfg(not(windows))]
    {
        Err(PlatformError::Unavailable)
    }
}

/// Run `netsh` with already-split arguments.
///
/// # Errors
///
/// Returns [`PlatformError::NeedAdmin`] when the firewall service asks for
/// elevation.
pub fn run_netsh(args: &[String]) -> Result<(), PlatformError> {
    #[cfg(windows)]
    {
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        run_command("netsh", &borrowed)
    }
    #[cfg(not(windows))]
    {
        let _ = args;
        Err(PlatformError::Unavailable)
    }
}

/// Read Orchid firewall rules as JSON. Empty when PowerShell cannot run.
#[must_use]
pub fn firewall_rules_json() -> String {
    #[cfg(windows)]
    {
        windows_impl::firewall_rules_json()
    }
    #[cfg(not(windows))]
    {
        String::new()
    }
}

/// `true` when `path` is one of the Explorer history keys, or a subkey of one.
#[must_use]
pub fn registry_path_allowed(path: &str) -> bool {
    if path.is_empty() || path.contains("..") || path.contains('/') {
        return false;
    }
    ALLOWED.iter().any(|root| {
        if path.eq_ignore_ascii_case(root) {
            return true;
        }
        let Some(rest) = path.get(root.len()..) else {
            return false;
        };
        path.is_char_boundary(root.len())
            && path[..root.len()].eq_ignore_ascii_case(root)
            && rest.starts_with('\\')
    })
}

const ALLOWED: &[&str] = &[
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\WordWheelQuery",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\TypedPaths",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\RecentDocs",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\OpenSavePidlMRU",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\ComDlg32\LastVisitedPidlMRU",
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist",
];

#[cfg(windows)]
fn run_command(program: &str, args: &[&str]) -> Result<(), PlatformError> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|e| PlatformError::Other(e.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    Err(classify_output(&output.stdout, &output.stderr))
}

#[cfg(windows)]
fn classify_output(stdout: &[u8], stderr: &[u8]) -> PlatformError {
    let text = format!(
        "{} {}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let lower = text.to_ascii_lowercase();
    if lower.contains("elevation")
        || lower.contains("access is denied")
        || lower.contains("отказано")
        || lower.contains("повышен")
    {
        PlatformError::NeedAdmin
    } else {
        let trimmed = text.trim();
        let short = trimmed.chars().take(180).collect::<String>();
        if short.is_empty() {
            PlatformError::Other("command failed".into())
        } else {
            PlatformError::Other(short)
        }
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{classify_output, registry_path_allowed, PlatformError};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY,
        HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE,
    };
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };

    pub fn empty_recycle() -> Result<(), PlatformError> {
        let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
        unsafe {
            SHEmptyRecycleBinW(None, PCWSTR::null(), flags).map_err(|err| map_win(err.code().0))?;
        }
        Ok(())
    }

    pub fn clear_registry_keys(keys: &[&str]) -> Result<u32, PlatformError> {
        let mut total = 0u32;
        for key in keys {
            total = total.saturating_add(clear_tree(key, 3)?);
        }
        Ok(total)
    }

    pub fn firewall_rules_json() -> String {
        let script = r#"
$ErrorActionPreference = 'SilentlyContinue'
$rows = Get-NetFirewallRule -DisplayName 'Orchid Protect*' | ForEach-Object {
  $app = $_ | Get-NetFirewallApplicationFilter
  [PSCustomObject]@{
    Name = $_.DisplayName
    Program = $app.Program
    Action = $_.Action.ToString()
    Direction = $_.Direction.ToString()
  }
}
if ($null -eq $rows) { '[]' } else { $rows | ConvertTo-Json -Compress }
"#;
        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output();
        match output {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Ok(output) => {
                let _ = classify_output(&output.stdout, &output.stderr);
                String::new()
            }
            Err(_) => String::new(),
        }
    }

    fn clear_tree(path: &str, depth: u8) -> Result<u32, PlatformError> {
        if !registry_path_allowed(path) {
            return Err(PlatformError::Other("registry path refused".into()));
        }
        let Some(key) = open_hkcu(path)? else {
            return Ok(0);
        };
        let mut removed = delete_values(key)?;
        if depth > 0 {
            let children = subkey_names(key);
            for child in children {
                if !safe_child(&child) {
                    continue;
                }
                let next = format!(r"{path}\{child}");
                removed = removed.saturating_add(clear_tree(&next, depth - 1)?);
            }
        }
        let _ = unsafe { RegCloseKey(key) };
        Ok(removed)
    }

    fn open_hkcu(path: &str) -> Result<Option<HKEY>, PlatformError> {
        let wide = wide(path);
        let mut key = HKEY::default();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(wide.as_ptr()),
                Some(0),
                KEY_READ | KEY_SET_VALUE,
                &mut key,
            )
        };
        match status.ok() {
            Ok(()) => Ok(Some(key)),
            Err(err) => {
                let code = err.code().0;
                if code == ERROR_FILE_NOT_FOUND.0 as i32 || (code as u32 & 0xFFFF) == 2 {
                    Ok(None)
                } else {
                    Err(map_win(code))
                }
            }
        }
    }

    fn delete_values(key: HKEY) -> Result<u32, PlatformError> {
        let names = value_names(key);
        let mut removed = 0u32;
        for name in names {
            let status = unsafe { RegDeleteValueW(key, PCWSTR(name.as_ptr())) };
            if status == ERROR_SUCCESS {
                removed = removed.saturating_add(1);
            }
        }
        Ok(removed)
    }

    fn value_names(key: HKEY) -> Vec<Vec<u16>> {
        let mut names = Vec::new();
        let mut index = 0u32;
        loop {
            if names.len() > 8_000 {
                break;
            }
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let mut ty = 0u32;
            let status = unsafe {
                RegEnumValueW(
                    key,
                    index,
                    Some(windows::core::PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    Some(&mut ty),
                    None,
                    None,
                )
            };
            if status == ERROR_NO_MORE_ITEMS || status != ERROR_SUCCESS {
                break;
            }
            index += 1;
            let mut name = buf[..len as usize].to_vec();
            name.push(0);
            names.push(name);
        }
        names
    }

    fn subkey_names(key: HKEY) -> Vec<String> {
        let mut names = Vec::new();
        let mut index = 0u32;
        loop {
            if names.len() > 512 {
                break;
            }
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            let status = unsafe {
                RegEnumKeyExW(
                    key,
                    index,
                    Some(windows::core::PWSTR(buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                )
            };
            if status == ERROR_NO_MORE_ITEMS || status != ERROR_SUCCESS {
                break;
            }
            index += 1;
            names.push(String::from_utf16_lossy(&buf[..len as usize]));
        }
        names
    }

    fn safe_child(name: &str) -> bool {
        !name.is_empty()
            && !name.contains('\\')
            && !name.contains('/')
            && name != "."
            && name != ".."
    }

    fn wide(text: &str) -> Vec<u16> {
        Path::new(text)
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn map_win(code: i32) -> PlatformError {
        let win = code as u32 & 0xFFFF;
        if win == 5 || code == ERROR_ACCESS_DENIED.0 as i32 {
            PlatformError::NeedAdmin
        } else {
            PlatformError::Other(format!("windows error {code}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_allowlist_rejects_other_keys() {
        assert!(registry_path_allowed(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU"
        ));
        assert!(registry_path_allowed(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\UserAssist\{75048700}\Count"
        ));
        assert!(!registry_path_allowed(
            r"Software\Microsoft\Windows\CurrentVersion\Run"
        ));
        assert!(!registry_path_allowed(
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU\..\Run"
        ));
        assert!(!registry_path_allowed(""));
    }
}
