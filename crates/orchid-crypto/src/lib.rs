//! Cryptography primitives for Orchid.
//!
//! This crate will expose:
//!
//! - file / folder encryption via the `age` crate,
//! - KDBX4 password-vault access via `keepass`,
//! - content-addressed chunking via `blake3` and `fastcdc`.

#![warn(missing_docs)]
#![warn(clippy::all)]

/// Returns the crate version as declared in `Cargo.toml`.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!version().is_empty());
    }
}
