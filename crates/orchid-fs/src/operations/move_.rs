//! Move operation: native rename when possible, fall back to copy + delete.

use tokio_util::sync::CancellationToken;

use crate::error::{FsError, Result};
use crate::operations::copy::{copy, CopyOptions};
use crate::operations::progress::ProgressSink;
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// Move `from` → `to`. If both endpoints live on the same local volume this
/// is a cheap `rename`; otherwise Orchid performs a full copy followed by a
/// delete of the source.
///
/// # Errors
///
/// Propagates any underlying provider / I/O error.
pub async fn move_(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    progress: Option<&ProgressSink>,
    cancel: Option<CancellationToken>,
) -> Result<()> {
    let src_provider = registry
        .for_path(from)
        .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?;

    // Fast path: same provider, same scheme, rename is cheap.
    if from.scheme() == to.scheme() {
        match src_provider.rename(from, to).await {
            Ok(()) => return Ok(()),
            Err(FsError::Io(e))
                if matches!(
                    e.raw_os_error(),
                    Some(17 /* EEXIST */) | Some(18 /* EXDEV */) | Some(5 /* EIO */)
                ) =>
            {
                // Fall back to copy + delete.
            }
            Err(FsError::AlreadyExists(_)) | Err(FsError::InvalidPath { .. }) => {
                return Err(FsError::AlreadyExists(to.to_string()));
            }
            // For any other error, retry via copy+delete to handle
            // cross-volume renames on Windows (ERROR_NOT_SAME_DEVICE = 17).
            Err(_) => {}
        }
    } else {
        if let Some(provider) = registry.for_path(from) {
            if provider
                .move_cross_scheme(registry, from, to, progress)
                .await?
            {
                return Ok(());
            }
        }
        if let Some(provider) = registry.for_path(to) {
            if provider
                .move_cross_scheme(registry, from, to, progress)
                .await?
            {
                return Ok(());
            }
        }
    }

    // Slow path: copy then delete source.
    let opts = CopyOptions {
        overwrite: false,
        verify_content_hash: false,
        preserve_timestamps: true,
        follow_symlinks: true,
    };
    copy(registry, from, to, opts, progress, cancel.clone()).await?;
    if let Some(c) = &cancel {
        if c.is_cancelled() {
            return Err(FsError::Cancelled);
        }
    }
    src_provider.remove(from, true).await?;
    Ok(())
}
