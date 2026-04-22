//! Slint-based UI layer for Orchid.
//!
//! Currently exposes the renderer-agnostic terminal-widget helpers
//! (palette, cell conversion, clipboard). The Slint component tree itself
//! is still a stub — the shared Theme global and window bootstrap land in a
//! follow-up task.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod widgets;

pub use widgets::terminal::{
    palette_from_flavor, snapshot_to_cells, ArboardClipboard, RenderCell, ThemeFlavor,
};

/// Returns the crate version as declared in `Cargo.toml`.
#[must_use]
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
