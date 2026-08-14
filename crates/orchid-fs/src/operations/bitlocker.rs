//! BitLocker status, lock, and unlock for a local volume.

use std::io;
use std::path::Path;

use crate::error::{FsError, Result};
use crate::path::FsPath;

/// Protection flag from `Win32_EncryptableVolume`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitLockerProtection {
    /// No key protectors / protection off.
    Off,
    /// Protection on.
    On,
    /// Status could not be read.
    Unknown,
}

/// Whether the volume is mounted unlocked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitLockerLock {
    /// Data is accessible.
    Unlocked,
    /// Volume is locked.
    Locked,
    /// Status could not be read.
    Unknown,
}

/// Conversion / wipe progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitLockerConversion {
    /// No encryption.
    FullyDecrypted,
    /// Fully encrypted.
    FullyEncrypted,
    /// Encryption in progress.
    Encrypting,
    /// Decryption in progress.
    Decrypting,
    /// Encryption paused.
    EncryptionPaused,
    /// Decryption paused.
    DecryptionPaused,
    /// Status could not be read.
    Unknown,
}

/// BitLocker state of the volume that contains a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitLockerStatus {
    /// Drive letter (`C:`).
    pub letter: String,
    /// Protection on/off.
    pub protection: BitLockerProtection,
    /// Locked vs unlocked.
    pub lock: BitLockerLock,
    /// Encryption conversion state.
    pub conversion: BitLockerConversion,
    /// 0–100 conversion percent.
    pub percent: u32,
    /// Cipher name (`XTS-AES 128`), empty if unknown.
    pub method: String,
}

/// BitLocker status for the volume of `path`.
///
/// # Errors
///
/// Non-local paths, missing WMI class, or access denied.
pub async fn bitlocker_status(path: &FsPath) -> Result<BitLockerStatus> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || bitlocker_status_os(&os))
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// Lock the BitLocker volume that contains `path`.
///
/// # Errors
///
/// Unencrypted volumes, access denied, or the OS volume cannot be locked.
pub async fn bitlocker_lock(path: &FsPath) -> Result<BitLockerStatus> {
    let os = path.to_local()?;
    tokio::task::spawn_blocking(move || bitlocker_lock_os(&os))
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// Unlock with a password or 48-digit recovery key.
///
/// # Errors
///
/// Bad secret, access denied, or an unsupported platform.
pub async fn bitlocker_unlock(path: &FsPath, secret: &str) -> Result<BitLockerStatus> {
    let os = path.to_local()?;
    let secret = secret.to_string();
    tokio::task::spawn_blocking(move || bitlocker_unlock_os(&os, &secret))
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// Open the Windows BitLocker Drive Encryption control panel.
///
/// # Errors
///
/// Unsupported platform or the panel could not be started.
pub async fn open_bitlocker_os() -> Result<()> {
    tokio::task::spawn_blocking(open_bitlocker_os_blocking)
        .await
        .map_err(|e| FsError::Io(io::Error::other(e)))?
}

/// `true` when `secret` is a 48-digit BitLocker recovery key (dashes optional).
#[must_use]
pub fn looks_like_recovery_key(secret: &str) -> bool {
    let digits: String = secret.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.len() == 48
}

/// Format 48 digits as `XXXXXX-XXXXXX-…`.
#[must_use]
pub fn format_recovery_key(secret: &str) -> Option<String> {
    let digits: String = secret.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 48 {
        return None;
    }
    Some(
        digits
            .as_bytes()
            .chunks(6)
            .map(|c| std::str::from_utf8(c).unwrap_or("000000"))
            .collect::<Vec<_>>()
            .join("-"),
    )
}

/// Drive letter (`C:`) for a local path, if any.
#[must_use]
pub fn bitlocker_drive_letter(path: &Path) -> Option<String> {
    let raw = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\");
    let s = raw
        .strip_prefix(r"\\?\")
        .or_else(|| raw.strip_prefix(r"//?/"))
        .unwrap_or(&raw);
    let mut chars = s.chars();
    let letter = chars.next()?;
    if letter.is_ascii_alphabetic() && chars.next() == Some(':') {
        Some(format!("{}:", letter.to_ascii_uppercase()))
    } else {
        None
    }
}

fn map_protection(v: u32) -> BitLockerProtection {
    match v {
        0 => BitLockerProtection::Off,
        1 => BitLockerProtection::On,
        _ => BitLockerProtection::Unknown,
    }
}

fn map_lock(v: u32) -> BitLockerLock {
    match v {
        0 => BitLockerLock::Unlocked,
        1 => BitLockerLock::Locked,
        _ => BitLockerLock::Unknown,
    }
}

fn map_conversion(v: u32) -> BitLockerConversion {
    match v {
        0 => BitLockerConversion::FullyDecrypted,
        1 => BitLockerConversion::FullyEncrypted,
        2 => BitLockerConversion::Encrypting,
        3 => BitLockerConversion::Decrypting,
        4 => BitLockerConversion::EncryptionPaused,
        5 => BitLockerConversion::DecryptionPaused,
        _ => BitLockerConversion::Unknown,
    }
}

fn method_name(v: u32) -> String {
    match v {
        0 => String::new(),
        1 => "AES-128 Diffuser".into(),
        2 => "AES-256 Diffuser".into(),
        3 => "AES-128".into(),
        4 => "AES-256".into(),
        5 => "Hardware".into(),
        6 => "XTS-AES 128".into(),
        7 => "XTS-AES 256".into(),
        other => format!("#{other}"),
    }
}

fn map_return(code: u32) -> Result<()> {
    match code {
        0 => Ok(()),
        0x8031_0000 => Err(FsError::Io(io::Error::other("fm-error-bitlocker-locked"))),
        0x8031_0001 | 0x8031_0008 => Err(FsError::Io(io::Error::other(
            "fm-error-bitlocker-not-encrypted",
        ))),
        0x8031_0027 | 0x8031_0030 | 0x8031_0031 => {
            Err(FsError::Io(io::Error::other("fm-error-bitlocker-auth")))
        }
        5 | 0x8007_0005 | 0x8004_1003 => {
            Err(FsError::Io(io::Error::other("fm-error-bitlocker-denied")))
        }
        _ => Err(FsError::Io(io::Error::other(
            "fm-error-bitlocker-unsupported",
        ))),
    }
}

#[cfg(windows)]
fn bitlocker_status_os(path: &Path) -> Result<BitLockerStatus> {
    let letter = bitlocker_drive_letter(path)
        .ok_or_else(|| FsError::Io(io::Error::other("fm-error-bitlocker-none")))?;
    with_bitlocker(&letter, |svc, device| read_status(svc, &letter, device))
}

#[cfg(not(windows))]
fn bitlocker_status_os(_path: &Path) -> Result<BitLockerStatus> {
    Err(FsError::Io(io::Error::other(
        "fm-error-bitlocker-unsupported",
    )))
}

#[cfg(windows)]
fn bitlocker_lock_os(path: &Path) -> Result<BitLockerStatus> {
    let letter = bitlocker_drive_letter(path)
        .ok_or_else(|| FsError::Io(io::Error::other("fm-error-bitlocker-none")))?;
    with_bitlocker(&letter, |svc, device| {
        let path = instance_path(device);
        let ins = method_in_params(svc, "Lock")?;
        put_bool(&ins, windows::core::w!("ForceDismount"), true)?;
        let out = exec_method(svc, &path, "Lock", Some(&ins))?;
        map_return(unsafe { wmi_u32(&out, windows::core::w!("ReturnValue")).unwrap_or(1) })?;
        read_status(svc, &letter, device)
    })
}

#[cfg(not(windows))]
fn bitlocker_lock_os(_path: &Path) -> Result<BitLockerStatus> {
    Err(FsError::Io(io::Error::other(
        "fm-error-bitlocker-unsupported",
    )))
}

#[cfg(windows)]
fn bitlocker_unlock_os(path: &Path, secret: &str) -> Result<BitLockerStatus> {
    let letter = bitlocker_drive_letter(path)
        .ok_or_else(|| FsError::Io(io::Error::other("fm-error-bitlocker-none")))?;
    let secret = secret.trim();
    if secret.is_empty() {
        return Err(FsError::Io(io::Error::other("fm-error-bitlocker-auth")));
    }
    with_bitlocker(&letter, |svc, device| {
        let path = instance_path(device);
        let (method, out) = if let Some(key) = format_recovery_key(secret) {
            let ins = method_in_params(svc, "UnlockWithNumericalPassword")?;
            put_bstr(&ins, windows::core::w!("NumericalPassword"), &key)?;
            (
                "UnlockWithNumericalPassword",
                exec_method(svc, &path, "UnlockWithNumericalPassword", Some(&ins))?,
            )
        } else {
            let ins = method_in_params(svc, "UnlockWithPassphrase")?;
            put_bstr(&ins, windows::core::w!("Passphrase"), secret)?;
            (
                "UnlockWithPassphrase",
                exec_method(svc, &path, "UnlockWithPassphrase", Some(&ins))?,
            )
        };
        let _ = method;
        map_return(unsafe { wmi_u32(&out, windows::core::w!("ReturnValue")).unwrap_or(1) })?;
        read_status(svc, &letter, device)
    })
}

#[cfg(not(windows))]
fn bitlocker_unlock_os(_path: &Path, _secret: &str) -> Result<BitLockerStatus> {
    Err(FsError::Io(io::Error::other(
        "fm-error-bitlocker-unsupported",
    )))
}

#[cfg(windows)]
fn open_bitlocker_os_blocking() -> Result<()> {
    use std::os::windows::process::CommandExt;
    const DETACHED: u32 = 0x0000_0008;
    let launched = std::process::Command::new("control")
        .args(["/name", "Microsoft.BitLockerDriveEncryption"])
        .creation_flags(DETACHED)
        .spawn()
        .is_ok();
    if launched {
        return Ok(());
    }
    std::process::Command::new("explorer")
        .arg("shell:::{D9EF8727-CAC2-4C9A-9026-43F7D089F933}")
        .creation_flags(DETACHED)
        .spawn()
        .map(|_| ())
        .map_err(|_| FsError::Io(io::Error::other("fm-error-bitlocker-os")))
}

#[cfg(not(windows))]
fn open_bitlocker_os_blocking() -> Result<()> {
    Err(FsError::Io(io::Error::other(
        "fm-error-bitlocker-unsupported",
    )))
}

#[cfg(windows)]
fn instance_path(device_id: &str) -> String {
    let escaped = device_id.replace('\\', r"\\");
    format!("Win32_EncryptableVolume.DeviceID=\"{escaped}\"")
}

#[cfg(windows)]
fn read_status(
    svc: &windows::Win32::System::Wmi::IWbemServices,
    letter: &str,
    device: &str,
) -> Result<BitLockerStatus> {
    use windows::core::w;

    let path = instance_path(device);
    let inst = get_object(svc, &path)?;
    let protection = unsafe { wmi_u32(&inst, w!("ProtectionStatus")) }
        .map(map_protection)
        .unwrap_or(BitLockerProtection::Unknown);
    let mut lock = BitLockerLock::Unknown;
    let mut conversion = BitLockerConversion::Unknown;
    let mut percent = 0u32;
    let mut method = String::new();
    if let Ok(out) = exec_method(svc, &path, "GetLockStatus", None) {
        if let Some(v) = unsafe { wmi_u32(&out, w!("LockStatus")) } {
            lock = map_lock(v);
        }
    }
    if let Ok(out) = exec_method(svc, &path, "GetConversionStatus", None) {
        if let Some(v) = unsafe { wmi_u32(&out, w!("ConversionStatus")) } {
            conversion = map_conversion(v);
        }
        percent = unsafe { wmi_u32(&out, w!("EncryptionPercentage")) }.unwrap_or(0);
    }
    if let Ok(out) = exec_method(svc, &path, "GetEncryptionMethod", None) {
        if let Some(v) = unsafe { wmi_u32(&out, w!("EncryptionMethod")) } {
            method = method_name(v);
        }
    }
    Ok(BitLockerStatus {
        letter: letter.to_string(),
        protection,
        lock,
        conversion,
        percent,
        method,
    })
}

#[cfg(windows)]
fn with_bitlocker<T>(
    letter: &str,
    f: impl FnOnce(&windows::Win32::System::Wmi::IWbemServices, &str) -> Result<T>,
) -> Result<T> {
    let (_com, svc) = connect_bitlocker()?;
    let device = find_device(&svc, letter)?;
    f(&svc, &device)
}

#[cfg(windows)]
fn connect_bitlocker() -> Result<(ComGuard, windows::Win32::System::Wmi::IWbemServices)> {
    use windows::core::BSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
    };
    use windows::Win32::System::Wmi::{IWbemLocator, IWbemServices, WbemLocator};

    let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let guard = ComGuard(initialized);
    let locator: IWbemLocator =
        unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).map_err(map_com)? };
    let empty = BSTR::new();
    let services: IWbemServices = unsafe {
        locator
            .ConnectServer(
                &BSTR::from("ROOT\\CIMV2\\Security\\MicrosoftVolumeEncryption"),
                &empty,
                &empty,
                &empty,
                0,
                &empty,
                None,
            )
            .map_err(map_com)?
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
    Ok((guard, services))
}

#[cfg(windows)]
struct ComGuard(bool);

#[cfg(windows)]
impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}

#[cfg(windows)]
fn find_device(svc: &windows::Win32::System::Wmi::IWbemServices, letter: &str) -> Result<String> {
    use windows::core::{w, BSTR};
    use windows::Win32::System::Wmi::{
        IEnumWbemClassObject, IWbemClassObject, WBEM_FLAG_FORWARD_ONLY,
        WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE,
    };

    let enumerator: IEnumWbemClassObject = unsafe {
        svc.ExecQuery(
            &BSTR::from("WQL"),
            &BSTR::from("SELECT DriveLetter, DeviceID FROM Win32_EncryptableVolume"),
            WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
            None,
        )
        .map_err(map_com)?
    };
    let want = letter.trim_end_matches('\\').to_ascii_uppercase();
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
        let drive = unsafe { wmi_string(&obj, w!("DriveLetter")) }
            .trim_end_matches('\\')
            .to_ascii_uppercase();
        let device = unsafe { wmi_string(&obj, w!("DeviceID")) };
        if drive == want && !device.is_empty() {
            return Ok(device);
        }
    }
    Err(FsError::Io(io::Error::other("fm-error-bitlocker-none")))
}

#[cfg(windows)]
fn get_object(
    svc: &windows::Win32::System::Wmi::IWbemServices,
    path: &str,
) -> Result<windows::Win32::System::Wmi::IWbemClassObject> {
    use windows::core::BSTR;
    use windows::Win32::System::Wmi::{IWbemClassObject, WBEM_GENERIC_FLAG_TYPE};

    let mut obj: Option<IWbemClassObject> = None;
    unsafe {
        svc.GetObject(
            &BSTR::from(path),
            WBEM_GENERIC_FLAG_TYPE(0),
            None,
            Some(&mut obj),
            None,
        )
        .map_err(map_com)?;
    }
    obj.ok_or_else(|| FsError::Io(io::Error::other("fm-error-bitlocker-none")))
}

#[cfg(windows)]
fn method_in_params(
    svc: &windows::Win32::System::Wmi::IWbemServices,
    method: &str,
) -> Result<windows::Win32::System::Wmi::IWbemClassObject> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;

    let class = get_object(svc, "Win32_EncryptableVolume")?;
    let name: Vec<u16> = std::ffi::OsStr::new(method)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut in_sig = None;
    let mut out_sig = None;
    unsafe {
        class
            .GetMethod(PCWSTR(name.as_ptr()), 0, &mut in_sig, &mut out_sig)
            .map_err(map_com)?;
    }
    let sig =
        in_sig.ok_or_else(|| FsError::Io(io::Error::other("fm-error-bitlocker-unsupported")))?;
    unsafe { sig.SpawnInstance(0).map_err(map_com) }
}

#[cfg(windows)]
fn exec_method(
    svc: &windows::Win32::System::Wmi::IWbemServices,
    path: &str,
    method: &str,
    ins: Option<&windows::Win32::System::Wmi::IWbemClassObject>,
) -> Result<windows::Win32::System::Wmi::IWbemClassObject> {
    use windows::core::BSTR;
    use windows::Win32::System::Wmi::{IWbemClassObject, WBEM_GENERIC_FLAG_TYPE};

    let mut out: Option<IWbemClassObject> = None;
    match ins {
        Some(params) => unsafe {
            svc.ExecMethod(
                &BSTR::from(path),
                &BSTR::from(method),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
                params,
                Some(&mut out),
                None,
            )
            .map_err(map_com)?;
        },
        None => unsafe {
            svc.ExecMethod(
                &BSTR::from(path),
                &BSTR::from(method),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
                None,
                Some(&mut out),
                None,
            )
            .map_err(map_com)?;
        },
    }
    out.ok_or_else(|| FsError::Io(io::Error::other("fm-error-bitlocker-unsupported")))
}

#[cfg(windows)]
fn put_bstr(
    obj: &windows::Win32::System::Wmi::IWbemClassObject,
    name: windows::core::PCWSTR,
    value: &str,
) -> Result<()> {
    use std::mem::ManuallyDrop;
    use windows::core::BSTR;
    use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR};

    let mut var = VARIANT::default();
    unsafe {
        let rec = &mut var.Anonymous.Anonymous;
        rec.vt = VT_BSTR;
        rec.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(value));
        let r = obj.Put(name, 0, &var, 0);
        let _ = VariantClear(&mut var);
        r.map_err(map_com)
    }
}

#[cfg(windows)]
fn put_bool(
    obj: &windows::Win32::System::Wmi::IWbemClassObject,
    name: windows::core::PCWSTR,
    value: bool,
) -> Result<()> {
    use windows::Win32::Foundation::VARIANT_BOOL;
    use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BOOL};

    let mut var = VARIANT::default();
    unsafe {
        let rec = &mut var.Anonymous.Anonymous;
        rec.vt = VT_BOOL;
        rec.Anonymous.boolVal = VARIANT_BOOL(if value { -1 } else { 0 });
        let r = obj.Put(name, 0, &var, 0);
        let _ = VariantClear(&mut var);
        r.map_err(map_com)
    }
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
unsafe fn wmi_u32(
    obj: &windows::Win32::System::Wmi::IWbemClassObject,
    name: windows::core::PCWSTR,
) -> Option<u32> {
    use windows::Win32::System::Variant::{VariantClear, VARIANT, VT_BSTR, VT_I4, VT_UI4};

    let mut var = VARIANT::default();
    let ok = unsafe { obj.Get(name, 0, &mut var, None, None) };
    let value = if ok.is_ok() {
        unsafe {
            let rec = &var.Anonymous.Anonymous;
            if rec.vt == VT_I4 {
                Some(rec.Anonymous.lVal as u32)
            } else if rec.vt == VT_UI4 {
                Some(rec.Anonymous.ulVal)
            } else if rec.vt == VT_BSTR {
                rec.Anonymous.bstrVal.to_string().parse().ok()
            } else {
                None
            }
        }
    } else {
        None
    };
    let _ = unsafe { VariantClear(&mut var) };
    value
}

#[cfg(windows)]
fn map_com(err: windows::core::Error) -> FsError {
    let hr = err.code().0 as u32;
    match hr {
        0x8007_0005 | 0x8004_1003 | 0x8007_000E => {
            FsError::Io(io::Error::other("fm-error-bitlocker-denied"))
        }
        _ => FsError::Io(io::Error::other("fm-error-bitlocker-unsupported")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_key_normalizes() {
        assert!(looks_like_recovery_key(
            "123456-123456-123456-123456-123456-123456-123456-123456"
        ));
        assert!(looks_like_recovery_key(&"1".repeat(48)));
        assert!(!looks_like_recovery_key("password"));
        assert_eq!(
            format_recovery_key(&"123456".repeat(8)).as_deref(),
            Some("123456-123456-123456-123456-123456-123456-123456-123456")
        );
    }

    #[test]
    fn drive_letter_from_windows_path() {
        assert_eq!(
            bitlocker_drive_letter(Path::new(r"C:\Users\foo")),
            Some("C:".into())
        );
        assert_eq!(
            bitlocker_drive_letter(Path::new(r"d:/data")),
            Some("D:".into())
        );
        assert_eq!(bitlocker_drive_letter(Path::new(r"\\server\share")), None);
    }

    #[test]
    fn maps_wmi_enums() {
        assert_eq!(map_protection(1), BitLockerProtection::On);
        assert_eq!(map_lock(1), BitLockerLock::Locked);
        assert_eq!(map_conversion(2), BitLockerConversion::Encrypting);
        assert_eq!(method_name(6), "XTS-AES 128");
        assert!(map_return(0).is_ok());
        assert!(map_return(0x8031_0027).is_err());
    }
}
