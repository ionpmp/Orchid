//! Windows Run-key sync for [`GeneralConfig::open_on_startup`].

use orchid_storage::GeneralConfig;

/// Sync the Windows login Run key with `open_on_startup`.
pub fn sync_open_on_startup(cfg: &GeneralConfig) {
    sync_open_on_startup_enabled(cfg.open_on_startup);
}

fn sync_open_on_startup_enabled(enabled: bool) {
    #[cfg(windows)]
    {
        if let Err(e) = sync_open_on_startup_windows(enabled) {
            tracing::warn!(?e, enabled, "failed to sync open-on-startup registry");
        }
        return;
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        tracing::debug!("open_on_startup registry sync is a no-op on this platform");
    }
}

/// Format an executable path for the Run key (`"path"` with quotes).
#[must_use]
pub fn quoted_startup_command(path: &str) -> String {
    format!("\"{path}\"")
}

#[cfg(windows)]
fn sync_open_on_startup_windows(enabled: bool) -> windows::core::Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows::core::w;
    use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    };

    if enabled {
        let exe = match std::env::current_exe() {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!(?e, "open_on_startup: could not resolve current executable");
                return Ok(());
            }
        };
        let quoted = quoted_startup_command(&exe.to_string_lossy());
        let wide: Vec<u16> = OsStr::new(&quoted)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
                w!("Orchid"),
                REG_SZ.0,
                Some(wide.as_ptr().cast()),
                (wide.len() * 2) as u32,
            )
            .ok()?;
        }
    } else {
        let status = unsafe {
            RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                w!(r"Software\Microsoft\Windows\CurrentVersion\Run"),
                w!("Orchid"),
            )
        };
        if status != ERROR_FILE_NOT_FOUND {
            status.ok()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_startup_command_wraps_path() {
        assert_eq!(
            quoted_startup_command(r"C:\Orchid\orchid.exe"),
            r#""C:\Orchid\orchid.exe""#
        );
    }

    #[test]
    fn sync_open_on_startup_accepts_default_general_config() {
        sync_open_on_startup(&GeneralConfig::default());
    }
}
