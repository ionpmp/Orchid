//! Core abstractions for Orchid.
//!
//! This crate will host the shared type vocabulary used across the workspace:
//! event-bus primitives, the command registry trait, and cross-cutting domain
//! types. For the current scaffolding stage it only exposes a version helper.

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
