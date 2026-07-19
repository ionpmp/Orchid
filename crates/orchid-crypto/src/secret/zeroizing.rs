//! Zeroing-on-drop byte buffers for in-memory secrets.

use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::Result;

/// A `Vec<u8>` that is wiped on drop.
///
/// # Security
///
/// Use for any in-memory secret: decrypted file contents, derived keys,
/// plaintext password fields. Never log, never serialise without explicit
/// opt-in, never clone without a clear security reason (there is no
/// `Clone` impl on purpose — use [`ZeroizingBytes::try_clone`] when you
/// must).
#[derive(Zeroize, ZeroizeOnDrop, Default)]
pub struct ZeroizingBytes(Vec<u8>);

impl ZeroizingBytes {
    /// Wrap an existing `Vec<u8>`. The vector is zeroed on drop.
    ///
    /// # Examples
    ///
    /// ```
    /// use orchid_crypto::ZeroizingBytes;
    /// let b = ZeroizingBytes::new(vec![1, 2, 3]);
    /// assert_eq!(b.as_slice(), &[1, 2, 3]);
    /// ```
    #[must_use]
    pub fn new(data: Vec<u8>) -> Self {
        Self(data)
    }

    /// Allocate an empty buffer with capacity hinted.
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self(Vec::with_capacity(cap))
    }

    /// Borrow the contents as a slice.
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Borrow the contents mutably.
    #[must_use]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0
    }

    /// Length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the buffer is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Append bytes to the buffer. Used by encryption output sinks.
    pub fn extend_from_slice(&mut self, other: &[u8]) {
        self.0.extend_from_slice(other);
    }

    /// Consume the buffer without zeroing. Use with caution — the returned
    /// `Vec` is no longer protected.
    #[must_use]
    pub fn into_inner(mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }

    /// Explicit clone. Returning a `Result` is deliberate: cloning a secret
    /// is a security-meaningful action and callers should think twice.
    ///
    /// # Errors
    ///
    /// Never errors in the current implementation; the signature reserves
    /// room for future failure modes (e.g. exceeding a configured cap).
    pub fn try_clone(&self) -> Result<Self> {
        Ok(Self(self.0.clone()))
    }
}

impl std::fmt::Debug for ZeroizingBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ZeroizingBytes({} bytes)", self.0.len())
    }
}

impl From<Vec<u8>> for ZeroizingBytes {
    fn from(v: Vec<u8>) -> Self {
        Self::new(v)
    }
}

/// Typed alias for passing in-memory byte secrets through APIs.
///
/// Wraps [`ZeroizingBytes`] in [`secrecy::SecretBox`] so that `Debug` never
/// prints the contents and access is always explicit via `ExposeSecret`.
pub type SecretBytes = secrecy::SecretBox<ZeroizingBytes>;

// `secrecy::SecretBox<T>` requires `T: Zeroize`. Our derives satisfy that.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_leak_contents() {
        let b = ZeroizingBytes::new(b"super-secret".to_vec());
        let dbg = format!("{b:?}");
        assert!(!dbg.contains("super-secret"));
        assert!(dbg.contains("12 bytes"));
    }

    #[test]
    fn into_inner_consumes_without_zeroing() {
        let b = ZeroizingBytes::new(vec![1, 2, 3, 4, 5]);
        let v = b.into_inner();
        assert_eq!(v, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn try_clone_produces_independent_copy() {
        let b = ZeroizingBytes::new(vec![9, 9, 9]);
        let c = b.try_clone().unwrap();
        assert_eq!(b.as_slice(), c.as_slice());
    }
}
