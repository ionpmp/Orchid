//! Identity types for [`age`]-based encryption.

use std::str::FromStr;
use std::sync::Arc;

use age::x25519;
use secrecy::SecretString;
#[cfg(test)]
use secrecy::ExposeSecret;

use crate::error::{CryptoError, Result};

/// How the user authenticates a file operation.
///
/// Either a passphrase (internally key-derived via scrypt by the age crate)
/// or an X25519 keypair. The `age::x25519::Identity` is wrapped in an `Arc`
/// because it is not easy to clone otherwise and we need to hand independent
/// copies to encryptor / decryptor tasks.
#[derive(Clone)]
pub enum Identity {
    /// Symmetric passphrase authentication.
    Passphrase(SecretString),
    /// X25519 recipient / identity pair.
    X25519(Arc<x25519::Identity>),
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Passphrase(_) => f.write_str("Identity::Passphrase(<redacted>)"),
            Self::X25519(_) => f.write_str("Identity::X25519(<redacted>)"),
        }
    }
}

impl Identity {
    /// Build from a plain string passphrase.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_crypto::Identity;
    /// let _ = Identity::passphrase("correct horse battery staple");
    /// ```
    #[must_use]
    pub fn passphrase(pw: impl Into<String>) -> Self {
        Self::Passphrase(SecretString::from(pw.into()))
    }

    /// Generate a fresh X25519 identity from the OS RNG.
    #[must_use]
    pub fn generate_x25519() -> Self {
        Self::X25519(Arc::new(x25519::Identity::generate()))
    }

    /// Parse an X25519 secret key encoded as `AGE-SECRET-KEY-1...`.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::Encoding`] if the string is not a valid
    /// secret key.
    pub fn x25519_from_secret_string(s: &str) -> Result<Self> {
        let ident = x25519::Identity::from_str(s)
            .map_err(|e| CryptoError::Encoding(format!("invalid AGE-SECRET-KEY: {e}")))?;
        Ok(Self::X25519(Arc::new(ident)))
    }

    /// Tag describing the underlying kind, for inclusion in metadata.
    #[must_use]
    pub fn kind(&self) -> super::metadata::IdentityKind {
        match self {
            Self::Passphrase(_) => super::metadata::IdentityKind::Passphrase,
            Self::X25519(_) => super::metadata::IdentityKind::X25519,
        }
    }

    /// Expose a passphrase identity as `&str` for diagnostic purposes.
    /// Intentionally kept test-only.
    #[cfg(test)]
    pub(crate) fn peek_passphrase(&self) -> Option<&str> {
        match self {
            Self::Passphrase(pw) => Some(pw.expose_secret()),
            Self::X25519(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_leak_secret() {
        let id = Identity::passphrase("open-sesame");
        let dbg = format!("{id:?}");
        assert!(!dbg.contains("open-sesame"));
        assert!(dbg.contains("redacted"));
    }

    #[test]
    fn x25519_round_trip() {
        let id = Identity::generate_x25519();
        let Identity::X25519(wrapped) = &id else {
            panic!("wrong variant");
        };
        let secret = wrapped.to_string();
        let restored = Identity::x25519_from_secret_string(secret.expose_secret()).unwrap();
        let Identity::X25519(restored_inner) = restored else {
            panic!("wrong variant");
        };
        assert_eq!(
            wrapped.to_public().to_string(),
            restored_inner.to_public().to_string()
        );
    }

    #[test]
    fn passphrase_peek_works_in_tests_only() {
        let id = Identity::passphrase("hunter2");
        assert_eq!(id.peek_passphrase(), Some("hunter2"));
    }
}
