//! Delete operations: permanent or to the OS recycle bin.

use crate::error::{FsError, Result};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// Delete tunables.
#[derive(Debug, Clone, Copy)]
pub struct DeleteOptions {
    /// When true, send to the OS recycle bin via the `trash` crate.
    pub to_recycle_bin: bool,
    /// When true, remove directory contents recursively.
    pub recursive: bool,
}

impl Default for DeleteOptions {
    fn default() -> Self {
        Self {
            to_recycle_bin: true,
            recursive: false,
        }
    }
}

/// Remove `path`. For recycle-bin deletes the path must live on the default
/// local provider.
///
/// # Errors
///
/// Propagates provider / OS errors.
pub async fn delete(
    registry: &FsProviderRegistry,
    path: &FsPath,
    options: DeleteOptions,
) -> Result<()> {
    if options.to_recycle_bin {
        let os_path = path.to_local()?;
        return tokio::task::spawn_blocking(move || {
            trash::delete(&os_path).map_err(|e| FsError::Io(std::io::Error::other(e)))
        })
        .await
        .map_err(|e| FsError::Io(std::io::Error::other(e)))?;
    }

    let provider = registry
        .for_path(path)
        .ok_or_else(|| FsError::ProviderNotMounted(path.to_string()))?;
    provider.remove(path, options.recursive).await
}
