//! Per-user replacement of the Windows sign-in shell.
//!
//! Writes `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Winlogon\Shell`
//! only. The machine key is never opened. An empty remembered value deletes
//! the per-user `Shell` string so Windows falls back to the machine default.

use std::path::Path;

use orchid_storage::{ConfigLoader, ShellConfig};

/// HKCU subkey that holds the per-user sign-in shell.
const WINLOGON_SUBKEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Winlogon";
const SHELL_VALUE: &str = "Shell";

/// Registry write chosen by [`plan_user_shell`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShellWrite {
    Unchanged,
    Set(String),
    Delete,
}

/// Next registry write and the `previous` string to store in config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellPlan {
    pub action: ShellWrite,
    pub previous: String,
}

/// True when `argv` contains `--restore-shell`.
#[must_use]
pub fn restore_shell_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter()
        .any(|arg| arg.as_ref() == "--restore-shell")
}

/// Clear `[shell].replace` and put the previous per-user shell back.
///
/// Call this before the single-instance check. A running Orchid shell would
/// otherwise treat the process as a second instance and exit before the
/// registry write. An empty remembered value deletes the HKCU `Shell` value.
///
/// # Errors
///
/// Returns an error when the config file cannot be loaded or saved.
pub fn restore_winlogon_shell(config_file: &Path) -> Result<(), String> {
    let mut cfg = ConfigLoader::load_or_create(config_file).map_err(|e| e.to_string())?;
    cfg.shell.replace = false;
    let _remembered = sync_shell(&mut cfg.shell);
    #[cfg(not(windows))]
    cfg.shell.previous.clear();
    ConfigLoader::save(&cfg, config_file).map_err(|e| e.to_string())
}

/// Apply `shell` to the HKCU Winlogon value. Returns whether `previous` changed.
pub(crate) fn sync_shell(shell: &mut ShellConfig) -> bool {
    #[cfg(windows)]
    {
        sync_shell_windows(shell)
    }
    #[cfg(not(windows))]
    {
        let _ = shell;
        false
    }
}

/// Decide the HKCU write from the saved flag, this executable, and the live value.
#[must_use]
pub(crate) fn plan_user_shell(
    replace: bool,
    exe_path: &str,
    current: Option<&str>,
    remembered: &str,
) -> ShellPlan {
    let current_text = current.map(str::trim).filter(|value| !value.is_empty());
    let remembered = remembered.trim();
    if replace {
        if current_text.is_some_and(|value| shell_points_at(value, exe_path)) {
            return ShellPlan {
                action: ShellWrite::Unchanged,
                previous: remembered.to_string(),
            };
        }
        let previous = current_text
            .map(str::to_string)
            .unwrap_or_else(|| remembered.to_string());
        return ShellPlan {
            action: ShellWrite::Set(crate::autostart::quoted_startup_command(exe_path)),
            previous,
        };
    }
    let ours = current_text.is_some_and(|value| shell_points_at(value, exe_path));
    if !ours {
        return ShellPlan {
            action: ShellWrite::Unchanged,
            previous: String::new(),
        };
    }
    if remembered.is_empty() || shell_points_at(remembered, exe_path) {
        ShellPlan {
            action: ShellWrite::Delete,
            previous: String::new(),
        }
    } else {
        ShellPlan {
            action: ShellWrite::Set(remembered.to_string()),
            previous: String::new(),
        }
    }
}

/// First command token, with surrounding quotes removed.
#[must_use]
fn first_shell_token(value: &str) -> &str {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('"') {
        let end = rest.find('"').unwrap_or(rest.len());
        return rest[..end].trim();
    }
    value.split_whitespace().next().unwrap_or("")
}

#[must_use]
fn shell_points_at(value: &str, exe_path: &str) -> bool {
    let exe = exe_path.trim().trim_matches('"');
    !exe.is_empty() && first_shell_token(value).eq_ignore_ascii_case(exe)
}

#[cfg(windows)]
fn sync_shell_windows(shell: &mut ShellConfig) -> bool {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            tracing::warn!(?e, "sign-in shell: could not resolve the executable");
            return false;
        }
    };
    let exe_path = exe.to_string_lossy();
    let current = match read_sz(WINLOGON_SUBKEY, SHELL_VALUE) {
        Ok(value) => value,
        Err(e) => {
            tracing::warn!(error = %e, "sign-in shell: could not read the per-user value");
            return false;
        }
    };
    let plan = plan_user_shell(
        shell.replace,
        &exe_path,
        current.as_deref(),
        &shell.previous,
    );
    let changed = plan.previous != shell.previous;
    if changed {
        shell.previous = plan.previous;
    }
    if let Err(e) = apply_action(WINLOGON_SUBKEY, SHELL_VALUE, &plan.action) {
        tracing::warn!(error = %e, "sign-in shell: registry write failed");
    }
    changed
}

#[cfg(windows)]
fn apply_action(subkey: &str, name: &str, action: &ShellWrite) -> Result<(), String> {
    match action {
        ShellWrite::Unchanged => Ok(()),
        ShellWrite::Set(value) => write_sz(subkey, name, value),
        ShellWrite::Delete => delete_value(subkey, name),
    }
}

#[cfg(windows)]
fn wide_null(text: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    OsStr::new(text)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(windows)]
fn read_sz(subkey: &str, name: &str) -> Result<Option<String>, String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA};
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};

    let key = wide_null(subkey);
    let value = wide_null(name);
    let mut buf = vec![0u16; 4096];
    let mut size = (buf.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if status == ERROR_MORE_DATA {
        return Err("per-user shell value is too long".to_string());
    }
    status.ok().map_err(|e| e.to_string())?;
    let chars = (size as usize) / 2;
    if chars == 0 {
        return Ok(None);
    }
    let end = (chars - 1).min(buf.len());
    let text = String::from_utf16_lossy(&buf[..end]);
    let text = text.trim().trim_end_matches('\0').trim();
    if text.is_empty() {
        Ok(None)
    } else {
        Ok(Some(text.to_string()))
    }
}

#[cfg(windows)]
fn write_sz(subkey: &str, name: &str, data: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

    let data = data.replace('\0', "");
    let key = wide_null(subkey);
    let value = wide_null(name);
    let wide = wide_null(&data);
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            REG_SZ.0,
            Some(wide.as_ptr().cast()),
            (wide.len() * 2) as u32,
        )
        .ok()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(windows)]
fn delete_value(subkey: &str, name: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{RegDeleteKeyValueW, HKEY_CURRENT_USER};

    let key = wide_null(subkey);
    let value = wide_null(name);
    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(());
    }
    status.ok().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Program Files\Orchid\orchid.exe";

    #[test]
    fn restore_flag_matches_only_that_argument() {
        assert!(restore_shell_requested(["--restore-shell"]));
        assert!(restore_shell_requested(["file.txt", "--restore-shell"]));
        assert!(!restore_shell_requested(["--restore-shell-now"]));
        assert!(!restore_shell_requested(["orchid.exe"]));
    }

    #[test]
    fn enabling_remembers_the_current_shell_and_quotes_orchid() {
        let plan = plan_user_shell(true, EXE, Some("explorer.exe"), "");
        assert_eq!(
            plan.action,
            ShellWrite::Set(r#""C:\Program Files\Orchid\orchid.exe""#.to_string())
        );
        assert_eq!(plan.previous, "explorer.exe");
    }

    #[test]
    fn enabling_when_already_ours_keeps_the_remembered_shell() {
        let current = r#""C:\Program Files\Orchid\orchid.exe" --flag"#;
        let plan = plan_user_shell(true, EXE, Some(current), "explorer.exe");
        assert_eq!(plan.action, ShellWrite::Unchanged);
        assert_eq!(plan.previous, "explorer.exe");
    }

    #[test]
    fn enabling_with_no_per_user_value_deletes_on_the_way_back() {
        let plan = plan_user_shell(true, EXE, None, "");
        assert!(matches!(plan.action, ShellWrite::Set(_)));
        assert_eq!(plan.previous, "");
        let back = plan_user_shell(
            false,
            EXE,
            Some(r#""C:\Program Files\Orchid\orchid.exe""#),
            "",
        );
        assert_eq!(back.action, ShellWrite::Delete);
        assert_eq!(back.previous, "");
    }

    #[test]
    fn disabling_writes_the_remembered_shell_back() {
        let plan = plan_user_shell(
            false,
            EXE,
            Some(r#""c:\program files\orchid\orchid.exe""#),
            "explorer.exe",
        );
        assert_eq!(plan.action, ShellWrite::Set("explorer.exe".to_string()));
        assert_eq!(plan.previous, "");
    }

    #[test]
    fn disabling_does_not_replace_a_shell_we_do_not_own() {
        let plan = plan_user_shell(false, EXE, Some(r"C:\Other\shell.exe"), "explorer.exe");
        assert_eq!(plan.action, ShellWrite::Unchanged);
        assert_eq!(plan.previous, "");
    }

    #[test]
    fn disabling_drops_a_remembered_value_that_points_at_orchid() {
        let plan = plan_user_shell(
            false,
            EXE,
            Some(r#""C:\Program Files\Orchid\orchid.exe""#),
            r#""C:\Program Files\Orchid\orchid.exe""#,
        );
        assert_eq!(plan.action, ShellWrite::Delete);
    }

    #[cfg(windows)]
    #[test]
    fn registry_round_trip_uses_a_private_key() {
        use windows::core::PCWSTR;
        use windows::Win32::System::Registry::{RegDeleteKeyW, HKEY_CURRENT_USER};

        let key = format!(r"Software\Orchid\shell-roundtrip-{}", std::process::id());
        assert_ne!(
            key, WINLOGON_SUBKEY,
            "the test key must not be the real sign-in shell"
        );
        assert!(WINLOGON_SUBKEY.contains(r"CurrentVersion\Winlogon"));
        struct DeleteKey(String);
        impl Drop for DeleteKey {
            fn drop(&mut self) {
                let wide = wide_null(&self.0);
                unsafe {
                    let _status = RegDeleteKeyW(HKEY_CURRENT_USER, PCWSTR(wide.as_ptr()));
                }
            }
        }
        let _guard = DeleteKey(key.clone());
        apply_action(&key, "Shell", &ShellWrite::Set("explorer.exe".to_string())).unwrap();
        assert_eq!(
            read_sz(&key, "Shell").unwrap().as_deref(),
            Some("explorer.exe")
        );
        apply_action(&key, "Shell", &ShellWrite::Delete).unwrap();
        assert_eq!(read_sz(&key, "Shell").unwrap(), None);
    }
}
