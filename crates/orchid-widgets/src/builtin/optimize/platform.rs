//! Live registry reads and writes. Other systems report the tweak unsupported.

use super::catalog::{ApplyStatus, Probe, RegOp};

/// Whether `probe` matches the current machine.
#[must_use]
pub fn probe_matches(probe: &Probe) -> bool {
    #[cfg(windows)]
    {
        win::probe_matches(probe)
    }
    #[cfg(not(windows))]
    {
        let _ = probe;
        false
    }
}

/// Write `ops`. Machine values that the user cannot change are imported elevated.
pub fn apply_ops(ops: &[RegOp]) -> ApplyStatus {
    #[cfg(windows)]
    {
        win::apply_ops(ops)
    }
    #[cfg(not(windows))]
    {
        let _ = ops;
        ApplyStatus::Unsupported
    }
}

#[cfg(windows)]
mod win {
    use std::path::Path;
    use std::process::Command;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteKeyValueW, RegDeleteTreeW, RegGetValueW, RegOpenKeyExW,
        RegSetKeyValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_DWORD, REG_SZ,
        RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    use super::super::catalog::{render_reg, ApplyStatus, Hive, Probe, RegOp};

    pub fn probe_matches(probe: &Probe) -> bool {
        match *probe {
            Probe::DwordEq {
                hive,
                key,
                name,
                value,
            } => read_dword(hive, key, name) == Some(value),
            Probe::SzEq {
                hive,
                key,
                name,
                value,
            } => read_sz(hive, key, name).as_deref() == Some(value),
            Probe::KeyExists { hive, key } => key_exists(hive, key),
        }
    }

    pub fn apply_ops(ops: &[RegOp]) -> ApplyStatus {
        let user: Vec<RegOp> = ops
            .iter()
            .copied()
            .filter(|op| hive_of(*op) == Hive::Cu)
            .collect();
        let machine: Vec<RegOp> = ops
            .iter()
            .copied()
            .filter(|op| hive_of(*op) == Hive::Lm)
            .collect();
        if let Err(status) = apply_direct(&user) {
            return status;
        }
        match apply_direct(&machine) {
            Ok(()) => ApplyStatus::Applied,
            Err(ApplyStatus::Denied) => elevate_import(&machine),
            Err(status) => status,
        }
    }

    fn apply_direct(ops: &[RegOp]) -> Result<(), ApplyStatus> {
        for op in ops {
            write_op(*op)?;
        }
        Ok(())
    }

    fn write_op(op: RegOp) -> Result<(), ApplyStatus> {
        match op {
            RegOp::Dword {
                hive,
                key,
                name,
                value,
            } => set_dword(hive, key, name, value),
            RegOp::Sz {
                hive,
                key,
                name,
                value,
            } => set_sz(hive, key, name, value),
            RegOp::DeleteValue { hive, key, name } => delete_value(hive, key, name),
            RegOp::EmptyDefault { hive, key } => set_sz(hive, key, "", ""),
            RegOp::DeleteKey { hive, key } => delete_tree(hive, key),
        }
    }

    fn elevate_import(ops: &[RegOp]) -> ApplyStatus {
        if ops.is_empty() {
            return ApplyStatus::Applied;
        }
        let path =
            std::env::temp_dir().join(format!("orchid-optimize-{}.reg", uuid::Uuid::new_v4()));
        let bytes = utf16_reg(&render_reg(ops));
        if std::fs::write(&path, bytes).is_err() {
            return ApplyStatus::Failed;
        }
        let status = import_elevated(&path);
        let _ = std::fs::remove_file(&path);
        status
    }

    fn import_elevated(path: &Path) -> ApplyStatus {
        let shown = path.to_string_lossy().replace('\'', "''");
        let script = format!(
            "$p = Start-Process -FilePath reg.exe -ArgumentList @('import','{shown}') -Verb RunAs -Wait -PassThru -WindowStyle Hidden; if ($null -eq $p) {{ exit 1 }}; exit $p.ExitCode"
        );
        match Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", &script])
            .output()
        {
            Ok(output) if output.status.success() => ApplyStatus::Applied,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).to_ascii_lowercase();
                let stdout = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
                if stderr.contains("cancel") || stdout.contains("cancel") {
                    ApplyStatus::Denied
                } else {
                    tracing::warn!(
                        stderr = %stderr,
                        "elevated registry import did not succeed"
                    );
                    ApplyStatus::Denied
                }
            }
            Err(error) => {
                tracing::warn!(%error, "could not start the administrator prompt");
                ApplyStatus::Failed
            }
        }
    }

    fn utf16_reg(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    }

    fn hive_of(op: RegOp) -> Hive {
        match op {
            RegOp::Dword { hive, .. }
            | RegOp::Sz { hive, .. }
            | RegOp::DeleteValue { hive, .. }
            | RegOp::EmptyDefault { hive, .. }
            | RegOp::DeleteKey { hive, .. } => hive,
        }
    }

    fn hkey(hive: Hive) -> HKEY {
        match hive {
            Hive::Cu => HKEY_CURRENT_USER,
            Hive::Lm => HKEY_LOCAL_MACHINE,
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn map_status(status: windows::core::Result<()>) -> Result<(), ApplyStatus> {
        match status {
            Ok(()) => Ok(()),
            Err(error) if error.code() == ERROR_ACCESS_DENIED.to_hresult() => {
                Err(ApplyStatus::Denied)
            }
            Err(error) => {
                tracing::warn!(%error, "registry write failed");
                Err(ApplyStatus::Failed)
            }
        }
    }

    fn set_dword(hive: Hive, key: &str, name: &str, value: u32) -> Result<(), ApplyStatus> {
        let key_w = wide(key);
        let name_w = wide(name);
        let status = unsafe {
            RegSetKeyValueW(
                hkey(hive),
                PCWSTR(key_w.as_ptr()),
                PCWSTR(name_w.as_ptr()),
                REG_DWORD.0,
                Some((&value as *const u32).cast()),
                4,
            )
        };
        map_status(status.ok())
    }

    fn set_sz(hive: Hive, key: &str, name: &str, value: &str) -> Result<(), ApplyStatus> {
        let key_w = wide(key);
        let name_w = wide(name);
        let data = wide(value);
        let status = unsafe {
            RegSetKeyValueW(
                hkey(hive),
                PCWSTR(key_w.as_ptr()),
                PCWSTR(if name.is_empty() {
                    std::ptr::null()
                } else {
                    name_w.as_ptr()
                }),
                REG_SZ.0,
                Some(data.as_ptr().cast()),
                (data.len() * 2) as u32,
            )
        };
        map_status(status.ok())
    }

    fn delete_value(hive: Hive, key: &str, name: &str) -> Result<(), ApplyStatus> {
        let key_w = wide(key);
        let name_w = wide(name);
        let status = unsafe {
            RegDeleteKeyValueW(hkey(hive), PCWSTR(key_w.as_ptr()), PCWSTR(name_w.as_ptr()))
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        map_status(status.ok())
    }

    fn delete_tree(hive: Hive, key: &str) -> Result<(), ApplyStatus> {
        let key_w = wide(key);
        let status = unsafe { RegDeleteTreeW(hkey(hive), PCWSTR(key_w.as_ptr())) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        map_status(status.ok())
    }

    fn read_dword(hive: Hive, key: &str, name: &str) -> Option<u32> {
        let key_w = wide(key);
        let name_w = wide(name);
        let mut data = 0u32;
        let mut size = 4u32;
        let status = unsafe {
            RegGetValueW(
                hkey(hive),
                PCWSTR(key_w.as_ptr()),
                PCWSTR(name_w.as_ptr()),
                RRF_RT_REG_DWORD,
                None,
                Some((&mut data as *mut u32).cast()),
                Some(&mut size),
            )
        };
        status.ok().ok()?;
        Some(data)
    }

    fn read_sz(hive: Hive, key: &str, name: &str) -> Option<String> {
        let key_w = wide(key);
        let name_w = wide(name);
        let mut buf = vec![0u16; 64];
        let mut size = (buf.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                hkey(hive),
                PCWSTR(key_w.as_ptr()),
                PCWSTR(name_w.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        status.ok().ok()?;
        let chars = (size as usize / 2).saturating_sub(1).min(buf.len());
        Some(
            String::from_utf16_lossy(&buf[..chars])
                .trim_end_matches('\0')
                .to_string(),
        )
    }

    fn key_exists(hive: Hive, key: &str) -> bool {
        let key_w = wide(key);
        let mut opened = HKEY::default();
        let status = unsafe {
            RegOpenKeyExW(
                hkey(hive),
                PCWSTR(key_w.as_ptr()),
                Some(0),
                KEY_READ,
                &raw mut opened,
            )
        };
        if status.is_ok() {
            let _ = unsafe { RegCloseKey(opened) };
            true
        } else {
            false
        }
    }
}
