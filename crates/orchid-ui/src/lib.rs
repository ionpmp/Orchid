//! Slint-based UI layer for Orchid.
//!
//! The Slint component tree is compiled from `ui/` by `build.rs` and will be
//! exposed here via `slint::include_modules!()` once the UI grows beyond the
//! current stub.

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
