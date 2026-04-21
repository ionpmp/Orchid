//! Filesystem layer for Orchid.
//!
//! Provides the unified provider abstraction that sits behind the file manager:
//! a local filesystem backend, network providers (SFTP / SMB / WebDAV / FTP via
//! rclone), directory watching via `notify`, and chunk-based content-addressed
//! storage powered by [`orchid_crypto`] and [`orchid_storage`].

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
