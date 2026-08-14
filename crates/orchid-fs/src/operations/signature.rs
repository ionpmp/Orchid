//! Digital signature inspection (Authenticode on Windows).

use std::path::Path;

use crate::error::{FsError, Result};
use crate::path::FsPath;

/// Outcome of a signature check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureReport {
    /// Human-readable lines for the Properties / Signature dialog.
    pub lines: Vec<String>,
}

/// Inspect an Authenticode (or PE certificate-table) signature.
///
/// # Errors
///
/// Non-local paths or I/O while probing the file.
pub fn inspect_signature(path: &FsPath) -> Result<SignatureReport> {
    let os = path.to_local()?;
    inspect_signature_os(&os)
}

fn inspect_signature_os(path: &Path) -> Result<SignatureReport> {
    let mut lines = Vec::new();
    lines.push(format!("File: {}", path.display()));
    if let Some(kind) = pe_kind(path)? {
        lines.push(format!("Container: {kind}"));
        if pe_has_certificate_table(path)? {
            lines.push("Embedded certificate table: yes".into());
        } else {
            lines.push("Embedded certificate table: no".into());
        }
    } else {
        lines.push("Container: not a PE image".into());
    }

    #[cfg(windows)]
    {
        match authenticode_status(path) {
            Ok(status) => lines.push(format!("Authenticode: {status}")),
            Err(e) => lines.push(format!("Authenticode: {e}")),
        }
    }
    #[cfg(not(windows))]
    {
        lines.push("Authenticode: available on Windows only".into());
    }

    Ok(SignatureReport { lines })
}

fn pe_kind(path: &Path) -> Result<Option<&'static str>> {
    let mut header = [0_u8; 64];
    let n = match std::fs::File::open(path).and_then(|mut f| {
        use std::io::Read;
        f.read(&mut header)
    }) {
        Ok(n) => n,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(FsError::Io(e)),
        Err(e) => return Err(FsError::Io(e)),
    };
    if n < 2 || header[0] != b'M' || header[1] != b'Z' {
        return Ok(None);
    }
    Ok(Some("PE (MZ)"))
}

fn pe_has_certificate_table(path: &Path) -> Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let mut dos = [0_u8; 64];
    if f.read(&mut dos)? < 64 {
        return Ok(false);
    }
    if dos[0] != b'M' || dos[1] != b'Z' {
        return Ok(false);
    }
    let pe_off = u32::from_le_bytes(dos[0x3C..0x40].try_into().unwrap()) as u64;
    f.seek(SeekFrom::Start(pe_off))?;
    let mut sig = [0_u8; 4];
    if f.read(&mut sig)? < 4 || &sig != b"PE\0\0" {
        return Ok(false);
    }
    let mut coff = [0_u8; 20];
    if f.read(&mut coff)? < 20 {
        return Ok(false);
    }
    let opt_size = u16::from_le_bytes(coff[16..18].try_into().unwrap()) as usize;
    if opt_size < 2 {
        return Ok(false);
    }
    let mut opt = vec![0_u8; opt_size];
    if f.read(&mut opt)? < opt_size {
        return Ok(false);
    }
    let magic = u16::from_le_bytes(opt[0..2].try_into().unwrap());
    let dd_off = match magic {
        0x10B => 96,  // PE32
        0x20B => 112, // PE32+
        _ => return Ok(false),
    };
    // DataDirectory[4] = IMAGE_DIRECTORY_ENTRY_SECURITY (8 bytes: VA + Size)
    let entry = dd_off + 4 * 8;
    if opt.len() < entry + 8 {
        return Ok(false);
    }
    let size = u32::from_le_bytes(opt[entry + 4..entry + 8].try_into().unwrap());
    Ok(size > 0)
}

#[cfg(windows)]
#[allow(clippy::field_reassign_with_default)]
fn authenticode_status(path: &Path) -> Result<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{GUID, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UI_NONE,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(wide.as_ptr()),
        hFile: windows::Win32::Foundation::HANDLE::default(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut data = WINTRUST_DATA::default();
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwUnionChoice = WTD_CHOICE_FILE;
    data.Anonymous.pFile = std::ptr::addr_of_mut!(file_info);
    data.dwStateAction = WTD_STATEACTION_VERIFY;

    let mut action: GUID = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let status = unsafe {
        WinVerifyTrust(
            HWND::default(),
            std::ptr::addr_of_mut!(action),
            std::ptr::addr_of_mut!(data).cast(),
        )
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    let _ = unsafe {
        WinVerifyTrust(
            HWND::default(),
            std::ptr::addr_of_mut!(action),
            std::ptr::addr_of_mut!(data).cast(),
        )
    };

    // TRUST_E_NOSIGNATURE = 0x800B0100
    const TRUST_E_NOSIGNATURE: i32 = -0x7FF4_FF00;
    const TRUST_E_BAD_DIGEST: i32 = -0x7FF4_FEFB;
    const TRUST_E_SUBJECT_NOT_TRUSTED: i32 = -0x7FF4_FEFE;
    Ok(match status {
        0 => "trusted".into(),
        TRUST_E_NOSIGNATURE => "no signature".into(),
        TRUST_E_BAD_DIGEST => "hash mismatch".into(),
        TRUST_E_SUBJECT_NOT_TRUSTED => "not trusted".into(),
        other => format!("HRESULT 0x{:08X}", other as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_pe_has_no_certificate_table() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plain.txt");
        std::fs::write(&path, b"hello").unwrap();
        assert!(!pe_has_certificate_table(&path).unwrap());
        assert!(pe_kind(&path).unwrap().is_none());
    }

    #[test]
    fn mz_without_pe_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.bin");
        let mut bytes = vec![0_u8; 64];
        bytes[0] = b'M';
        bytes[1] = b'Z';
        bytes[0x3C] = 64;
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(pe_kind(&path).unwrap(), Some("PE (MZ)"));
        assert!(!pe_has_certificate_table(&path).unwrap());
    }
}
