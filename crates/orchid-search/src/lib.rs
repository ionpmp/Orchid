//! Search engine for Orchid.
//!
//! Wraps Tantivy to provide full-text and metadata search over indexed files,
//! with incremental re-indexing driven by `notify` watchers.

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
