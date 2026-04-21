//! Terminal emulation for Orchid.
//!
//! Combines `portable-pty` for process / PTY hosting with `alacritty_terminal`
//! for VT parsing and grid state. Support for inline graphics (sixel, kitty) is
//! planned and will be layered on top via `wezterm-term` in a later stage.

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
