//! Encode / decode short secrets for storage (e.g. network mount passwords).
//!
//! On Windows, values are DPAPI-protected and stored as `dpapi:<hex>`.
//! On other platforms (or if DPAPI is unavailable) the plaintext is left as-is
//! so config remains usable; callers should still prefer `rclone-remote`.

use crate::error::{CryptoError, Result};
use crate::secret::dpapi;

const DPAPI_PREFIX: &str = "dpapi:";

/// Returns `true` when `value` is a DPAPI-encoded blob produced by [`protect_for_storage`].
#[must_use]
pub fn is_protected(value: &str) -> bool {
    value.starts_with(DPAPI_PREFIX)
}

/// Protect `plaintext` for writing into config / disk.
///
/// On success returns `dpapi:<hex>`. If DPAPI is unavailable, returns the
/// original plaintext unchanged (so non-Windows builds keep working).
///
/// # Errors
///
/// Returns [`CryptoError::Dpapi`] when Win32 rejects the protect call.
pub fn protect_for_storage(plaintext: &str) -> Result<String> {
    if plaintext.is_empty() || is_protected(plaintext) {
        return Ok(plaintext.to_string());
    }
    match dpapi::protect(plaintext.as_bytes(), Some("orchid-network-mount")) {
        Ok(blob) => Ok(format!("{DPAPI_PREFIX}{}", hex::encode(blob))),
        Err(CryptoError::DpapiUnavailable) => Ok(plaintext.to_string()),
        Err(e) => Err(e),
    }
}

/// Resolve a stored secret to plaintext for use (e.g. rclone argv).
///
/// Plaintext values (legacy config) are returned unchanged. DPAPI blobs are
/// decrypted. Empty strings pass through.
///
/// # Errors
///
/// Returns [`CryptoError::Dpapi`] when the blob is corrupt or cannot be
/// decrypted, or when the hex payload is invalid.
pub fn resolve_stored_secret(value: &str) -> Result<String> {
    let Some(hex_blob) = value.strip_prefix(DPAPI_PREFIX) else {
        return Ok(value.to_string());
    };
    let blob = hex::decode(hex_blob)
        .map_err(|e| CryptoError::Dpapi(format!("invalid dpapi hex: {e}")))?;
    let plain = dpapi::unprotect(&blob)?;
    String::from_utf8(plain.into_inner()).map_err(|e| CryptoError::Dpapi(e.to_string()))
}

/// DPAPI-protect any plaintext [`NetworkMountConfig::password`] values in place.
///
/// Returns `true` when at least one password was rewritten. Failures to protect
/// leave the plaintext value unchanged and are logged by the caller via the
/// returned `Err` for the first failure (subsequent mounts are still attempted
/// only on success path — we stop on first hard DPAPI error).
///
/// # Errors
///
/// Propagates [`CryptoError::Dpapi`] from [`protect_for_storage`].
pub fn protect_network_mount_passwords(
    mounts: &mut [orchid_storage::NetworkMountConfig],
) -> Result<bool> {
    let mut changed = false;
    for mount in mounts.iter_mut() {
        let Some(pass) = mount.password.as_deref().filter(|p| !p.is_empty()) else {
            continue;
        };
        if is_protected(pass) {
            continue;
        }
        let protected = protect_for_storage(pass)?;
        if protected != pass {
            mount.password = Some(protected);
            changed = true;
        }
    }
    Ok(changed)
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn protect_resolve_round_trip() {
        let stored = protect_for_storage("s3cret").unwrap();
        assert!(is_protected(&stored));
        assert_ne!(stored, "s3cret");
        assert_eq!(resolve_stored_secret(&stored).unwrap(), "s3cret");
    }

    #[test]
    fn plaintext_passthrough() {
        assert_eq!(resolve_stored_secret("legacy").unwrap(), "legacy");
        assert!(!is_protected("legacy"));
    }
}
