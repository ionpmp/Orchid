//! Windows DPAPI wrapper.
//!
//! Backed by `CryptProtectData` / `CryptUnprotectData` with
//! `CRYPTPROTECT_UI_FORBIDDEN`.
//!
//! # Threat model
//!
//! DPAPI-protected blobs are safe against an offline attacker who does not
//! have access to the user's Windows profile. They do NOT protect against
//! malware running as the same user (such malware can simply call
//! `CryptUnprotectData` itself). Use this to store, for example, the unlock
//! key that grants access to a larger key vault in combination with a
//! biometric gate.
//!
//! On non-Windows targets both [`protect`] and [`unprotect`] return
//! [`crate::CryptoError::DpapiUnavailable`] without touching any OS API.

#[cfg(not(windows))]
use crate::error::CryptoError;
use crate::error::Result;
use crate::secret::zeroizing::ZeroizingBytes;

#[cfg(windows)]
mod imp {
    #![allow(unsafe_code)]
    // The `windows` crate's CryptProtectData / CryptUnprotectData wrappers
    // are marked `unsafe` because they ultimately hand raw pointers to
    // Win32. Every call below materialises its Rust-side inputs in owned
    // `Vec<u8>`s before the call, and copies / drops any Win32-allocated
    // buffers immediately after, so no unsynchronised aliasing is
    // possible. The `unsafe_code` attribute is relaxed for this module only.

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::LocalFree;
    use windows::Win32::Foundation::HLOCAL;
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    };

    use crate::error::{CryptoError, Result};
    use crate::secret::zeroizing::ZeroizingBytes;

    fn make_blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: data.len() as u32,
            pbData: data.as_ptr() as *mut u8,
        }
    }

    pub fn protect(plaintext: &[u8], description: Option<&str>) -> Result<Vec<u8>> {
        let in_blob = make_blob(plaintext);
        let mut out_blob = CRYPT_INTEGER_BLOB::default();

        // Description is optional; when provided it is encoded as a UTF-16
        // nul-terminated string whose pointer stays valid for the duration
        // of the call.
        let desc_utf16: Option<Vec<u16>> = description.map(|s| {
            let mut v: Vec<u16> = s.encode_utf16().collect();
            v.push(0);
            v
        });
        let desc_ptr = desc_utf16
            .as_ref()
            .map(|v| PCWSTR(v.as_ptr()))
            .unwrap_or(PCWSTR::null());

        // SAFETY: Win32 call. `in_blob.pbData` points into `plaintext`,
        // which outlives the call. `desc_ptr`, if non-null, points into
        // `desc_utf16`, which also outlives the call. On success Win32
        // allocates `out_blob.pbData` which we copy and free below.
        let ok = unsafe {
            CryptProtectData(
                &in_blob,
                desc_ptr,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        ok.map_err(|e| CryptoError::Dpapi(e.to_string()))?;

        let len = out_blob.cbData as usize;
        // SAFETY: Win32 populated `out_blob` with a heap-allocated buffer of
        // exactly `cbData` bytes. We copy into a fresh `Vec` and then free.
        let out = unsafe { std::slice::from_raw_parts(out_blob.pbData, len) }.to_vec();
        // SAFETY: Win32 allocated `out_blob.pbData` via LocalAlloc; freeing
        // with LocalFree is the matching operation.
        unsafe {
            let _ = LocalFree(HLOCAL(out_blob.pbData.cast()));
        }
        Ok(out)
    }

    pub fn unprotect(ciphertext: &[u8]) -> Result<ZeroizingBytes> {
        let in_blob = make_blob(ciphertext);
        let mut out_blob = CRYPT_INTEGER_BLOB::default();

        // SAFETY: same shape as `protect`.
        let ok = unsafe {
            CryptUnprotectData(
                &in_blob,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out_blob,
            )
        };
        ok.map_err(|e| CryptoError::Dpapi(e.to_string()))?;

        let len = out_blob.cbData as usize;
        // SAFETY: see `protect`.
        let plain = unsafe { std::slice::from_raw_parts(out_blob.pbData, len) }.to_vec();
        // Wipe the Win32 buffer before freeing so the plaintext never
        // lingers on the process heap.
        // SAFETY: Win32-allocated buffer of known length; we write over it
        // and then release it.
        unsafe {
            std::ptr::write_bytes(out_blob.pbData, 0u8, len);
            let _ = LocalFree(HLOCAL(out_blob.pbData.cast()));
        }
        Ok(ZeroizingBytes::new(plain))
    }
}

/// Protect `plaintext` so that only the current user on this machine can
/// later recover it via [`unprotect`].
///
/// On non-Windows targets this returns
/// [`crate::CryptoError::DpapiUnavailable`] without performing any work.
///
/// # Errors
///
/// Returns [`crate::CryptoError::Dpapi`] if Win32 rejects the call.
#[cfg(windows)]
pub fn protect(plaintext: &[u8], description: Option<&str>) -> Result<Vec<u8>> {
    imp::protect(plaintext, description)
}

/// Non-Windows no-op placeholder.
#[cfg(not(windows))]
pub fn protect(_plaintext: &[u8], _description: Option<&str>) -> Result<Vec<u8>> {
    Err(CryptoError::DpapiUnavailable)
}

/// Recover a buffer previously produced by [`protect`].
///
/// # Errors
///
/// Returns [`crate::CryptoError::Dpapi`] if the ciphertext is corrupted, was
/// produced on a different user / machine, or Win32 rejects the call.
#[cfg(windows)]
pub fn unprotect(ciphertext: &[u8]) -> Result<ZeroizingBytes> {
    imp::unprotect(ciphertext)
}

/// Non-Windows no-op placeholder.
#[cfg(not(windows))]
pub fn unprotect(_ciphertext: &[u8]) -> Result<ZeroizingBytes> {
    Err(CryptoError::DpapiUnavailable)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn dpapi_round_trip() {
        let protected = protect(b"super-secret-key", Some("orchid-test")).unwrap();
        assert_ne!(protected, b"super-secret-key");

        let recovered = unprotect(&protected).unwrap();
        assert_eq!(recovered.as_slice(), b"super-secret-key");
    }

    #[test]
    fn corrupted_ciphertext_errors() {
        let protected = protect(b"abc", None).unwrap();
        let mut tampered = protected;
        // Flip some bits in the middle where the DPAPI payload lives.
        let mid = tampered.len() / 2;
        tampered[mid] ^= 0xFF;
        assert!(unprotect(&tampered).is_err());
    }
}
