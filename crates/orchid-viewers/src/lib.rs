//! Viewers for Orchid.
//!
//! Hosts the per-format viewing logic: `pdfium-render` for PDFs, `image` for
//! raster formats, `tree-sitter` for syntax-highlighted text, and
//! `sevenz-rust` / `zip` for archive browsing. The crate exposes a single
//! viewer trait so the UI can route files by detected kind.

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
