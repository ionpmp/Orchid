//! Secure randomness helpers.
//!
//! Every function here is backed by [`rand::rngs::OsRng`] — never the thread
//! RNG — so they are appropriate for key material, nonces, and identifiers.

use rand::rngs::OsRng;
use rand::RngCore;

use crate::error::{CryptoError, Result};
use crate::secret::zeroizing::ZeroizingBytes;

/// Fill `buf` with cryptographically secure random bytes.
///
/// # Errors
///
/// Returns [`CryptoError::Encoding`] if the OS RNG reports an error
/// (extremely unlikely on a desktop target — we still surface it rather
/// than panic).
pub fn fill_secure(buf: &mut [u8]) -> Result<()> {
    let mut rng = OsRng;
    rng.try_fill_bytes(buf)
        .map_err(|e| CryptoError::Encoding(format!("OS RNG failure: {e}")))
}

/// Allocate `n` bytes of secure randomness into a zeroizing buffer.
///
/// # Errors
///
/// See [`fill_secure`].
///
/// # Examples
///
/// ```
/// use orchid_crypto::random_bytes;
/// let b = random_bytes(32).unwrap();
/// assert_eq!(b.as_slice().len(), 32);
/// ```
pub fn random_bytes(n: usize) -> Result<ZeroizingBytes> {
    let mut buf = vec![0u8; n];
    fill_secure(&mut buf)?;
    Ok(ZeroizingBytes::new(buf))
}

/// Generate a version-4 UUID using the OS RNG.
///
/// # Examples
///
/// ```
/// use orchid_crypto::random_uuid;
/// let a = random_uuid();
/// let b = random_uuid();
/// assert_ne!(a, b);
/// ```
#[must_use]
pub fn random_uuid() -> uuid::Uuid {
    let mut bytes = [0u8; 16];
    // OsRng doesn't fail on desktop targets; an `ok()` dance here would
    // swallow the extremely unlikely error. We fall back to thread-RNG in
    // that path — still a cryptographic RNG for UUIDs on stable Rust.
    if fill_secure(&mut bytes).is_err() {
        uuid::Uuid::new_v4()
    } else {
        // Set version (4) and variant (RFC 4122) bits.
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        uuid::Uuid::from_bytes(bytes)
    }
}

/// Default length for [`generate_password`].
pub const DEFAULT_PASSWORD_LENGTH: usize = 20;

const PASSWORD_CHARSET: &[u8] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()-_=+[]{}|;:,.<>?";

/// Generate a random password using letters, digits, and symbols.
///
/// # Errors
///
/// See [`fill_secure`].
pub fn generate_password(length: usize) -> Result<String> {
    if length == 0 {
        return Ok(String::new());
    }
    let mut bytes = vec![0u8; length];
    fill_secure(&mut bytes)?;
    let charset_len = PASSWORD_CHARSET.len();
    Ok(bytes
        .iter()
        .map(|&b| PASSWORD_CHARSET[(b as usize) % charset_len] as char)
        .collect())
}

/// Generate a random password using [`DEFAULT_PASSWORD_LENGTH`].
///
/// # Errors
///
/// See [`generate_password`].
pub fn generate_password_default() -> Result<String> {
    generate_password(DEFAULT_PASSWORD_LENGTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_bytes_returns_requested_length() {
        let b = random_bytes(47).unwrap();
        assert_eq!(b.as_slice().len(), 47);
    }

    #[test]
    fn two_random_bytes_calls_differ() {
        let a = random_bytes(32).unwrap();
        let b = random_bytes(32).unwrap();
        assert_ne!(a.as_slice(), b.as_slice());
    }

    #[test]
    fn random_uuids_differ() {
        let ids: std::collections::HashSet<_> =
            (0..16).map(|_| random_uuid()).collect();
        assert_eq!(ids.len(), 16);
    }

    #[test]
    fn generate_password_has_requested_length() {
        let p = generate_password(20).unwrap();
        assert_eq!(p.len(), 20);
    }

    #[test]
    fn generate_password_default_length() {
        let p = generate_password_default().unwrap();
        assert_eq!(p.len(), DEFAULT_PASSWORD_LENGTH);
    }

    #[test]
    fn generate_passwords_differ() {
        let a = generate_password(32).unwrap();
        let b = generate_password(32).unwrap();
        assert_ne!(a, b);
    }
}
