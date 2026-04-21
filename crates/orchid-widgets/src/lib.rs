//! Widget infrastructure for Orchid.
//!
//! Owns the widget lifecycle, layout persistence, and the traits that the
//! built-in widgets (weather, moon, system indicators, media player, RSS,
//! search) implement. Widget state and placement are persisted through
//! [`orchid_storage`], and shared types come from [`orchid_core`].

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
