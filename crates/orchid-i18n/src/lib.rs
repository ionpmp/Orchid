//! Localization for Orchid.
//!
//! Built on ICU for locale-aware formatting and collation, with message
//! catalogues stored as TOML under `locales/`. RTL layout decisions are
//! surfaced here so UI code stays locale-agnostic.

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
