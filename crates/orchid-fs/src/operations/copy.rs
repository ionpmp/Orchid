//! Copy operation with streaming, progress, cancellation, and optional
//! content-hash verification.

use std::path::PathBuf;

use filetime::FileTime;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::error::{FsError, Result};
use crate::operations::progress::{OperationProgress, ProgressSink};
use crate::path::FsPath;
use crate::provider::FsProviderRegistry;

/// Tunable knobs for [`copy`].
#[derive(Debug, Clone, Copy)]
pub struct CopyOptions {
    /// Overwrite an existing destination.
    pub overwrite: bool,
    /// Compute BLAKE3 on source + destination; fail if they differ.
    pub verify_content_hash: bool,
    /// Preserve `modified` (and where supported, `accessed`) timestamps.
    pub preserve_timestamps: bool,
    /// Follow symlinks rather than copying the link itself.
    pub follow_symlinks: bool,
}

impl Default for CopyOptions {
    fn default() -> Self {
        Self {
            overwrite: false,
            verify_content_hash: false,
            preserve_timestamps: true,
            follow_symlinks: true,
        }
    }
}

/// Copy `from` → `to`, recursing through directories.
///
/// Progress is emitted on `progress` (if provided) after every file. The
/// `cancel` token is checked at every file boundary.
///
/// # Errors
///
/// Propagates any underlying provider / I/O error;
/// [`FsError::AlreadyExists`] if the target exists and `overwrite` is
/// false; [`FsError::Cancelled`] if the token fires.
pub async fn copy(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    options: CopyOptions,
    progress: Option<&ProgressSink>,
    cancel: Option<CancellationToken>,
) -> Result<()> {
    let src_provider = registry
        .for_path(from)
        .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?;
    let _dst_provider = registry
        .for_path(to)
        .ok_or_else(|| FsError::ProviderNotMounted(to.to_string()))?;

    let src_meta = src_provider.metadata(from).await?;
    if matches!(src_meta.kind, crate::entry::FsEntryKind::Directory) {
        copy_directory(
            registry,
            from,
            to,
            options,
            progress,
            cancel,
        )
        .await
    } else {
        copy_file_with_progress(
            registry,
            from,
            to,
            options,
            progress,
            cancel,
            0,
            0,
        )
        .await?;
        Ok(())
    }
}

async fn copy_directory(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    options: CopyOptions,
    progress: Option<&ProgressSink>,
    cancel: Option<CancellationToken>,
) -> Result<()> {
    // MVP: directory copy only supported between local paths, since that's
    // the only provider that exists today. Walk with `walkdir`, stream each
    // file through the generic provider path.
    if !from.is_local() || !to.is_local() {
        return Err(FsError::InvalidPath {
            reason: "cross-provider directory copy is not supported in MVP".into(),
        });
    }
    let src_os = from.to_local()?;
    let dst_os = to.to_local()?;

    // First pass: enumerate work + total bytes so progress is meaningful.
    let mut files: Vec<(PathBuf, PathBuf, u64)> = Vec::new();
    let mut total_bytes: u64 = 0;
    for entry in walkdir::WalkDir::new(&src_os).follow_links(options.follow_symlinks) {
        let entry = entry?;
        let rel = entry.path().strip_prefix(&src_os).unwrap_or(entry.path());
        let dst = dst_os.join(rel);
        if entry.file_type().is_dir() {
            tokio::fs::create_dir_all(&dst).await?;
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        total_bytes += size;
        files.push((entry.path().to_path_buf(), dst, size));
    }

    let items_total = files.len() as u64;
    let mut bytes_done: u64 = 0;

    for (items_done, (src, dst, _size)) in (0_u64..).zip(files.into_iter()) {
        if let Some(c) = &cancel {
            if c.is_cancelled() {
                return Err(FsError::Cancelled);
            }
        }
        let src_fs = FsPath::from_local(&src)?;
        let dst_fs = FsPath::from_local(&dst)?;
        let bytes_copied = copy_file_with_progress(
            registry,
            &src_fs,
            &dst_fs,
            options,
            progress,
            cancel.clone(),
            bytes_done,
            total_bytes,
        )
        .await?;
        bytes_done += bytes_copied;
        if let Some(p) = progress {
            p.send(OperationProgress {
                total_bytes,
                processed_bytes: bytes_done,
                current_path: dst_fs,
                items_processed: items_done + 1,
                items_total,
            });
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn copy_file_with_progress(
    registry: &FsProviderRegistry,
    from: &FsPath,
    to: &FsPath,
    options: CopyOptions,
    progress: Option<&ProgressSink>,
    cancel: Option<CancellationToken>,
    bytes_before: u64,
    bytes_total: u64,
) -> Result<u64> {
    let src_provider = registry
        .for_path(from)
        .ok_or_else(|| FsError::ProviderNotMounted(from.to_string()))?;
    let dst_provider = registry
        .for_path(to)
        .ok_or_else(|| FsError::ProviderNotMounted(to.to_string()))?;

    if !options.overwrite && dst_provider.exists(to).await? {
        return Err(FsError::AlreadyExists(to.to_string()));
    }

    let src_meta = src_provider.metadata(from).await?;
    let total = if bytes_total == 0 { src_meta.size } else { bytes_total };

    let mut reader = src_provider.read_stream(from).await?;
    let mut writer = dst_provider.write_stream(to).await?;

    let mut hasher = if options.verify_content_hash {
        Some(orchid_crypto::StreamHasher::new())
    } else {
        None
    };

    let mut buf = vec![0u8; 128 * 1024];
    let mut written: u64 = 0;
    loop {
        if let Some(c) = &cancel {
            if c.is_cancelled() {
                return Err(FsError::Cancelled);
            }
        }
        let n = reader.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        if let Some(h) = &mut hasher {
            h.update(&buf[..n]);
        }
        writer.write_all(&buf[..n]).await?;
        written += n as u64;
        if let Some(p) = progress {
            p.send(OperationProgress {
                total_bytes: total,
                processed_bytes: bytes_before + written,
                current_path: to.clone(),
                items_processed: 0,
                items_total: 0,
            });
        }
    }
    writer.flush().await?;
    drop(writer);

    if let Some(h) = hasher {
        let src_hash = h.finalize();
        // Recompute hash of destination independently.
        let mut dst_reader = dst_provider.read_stream(to).await?;
        let mut dst_hasher = orchid_crypto::StreamHasher::new();
        loop {
            let n = dst_reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            dst_hasher.update(&buf[..n]);
        }
        let dst_hash = dst_hasher.finalize();
        if src_hash != dst_hash {
            // Remove the corrupted destination before surfacing the error.
            let _ = dst_provider.remove(to, false).await;
            return Err(FsError::EncryptedOp(
                "content-hash verification failed after copy".into(),
            ));
        }
    }

    if options.preserve_timestamps {
        if let (Some(modified), true) = (src_meta.modified, to.is_local()) {
            let os = to.to_local()?;
            let ft = FileTime::from_unix_time(modified.timestamp(), modified.timestamp_subsec_nanos());
            let _ = tokio::task::spawn_blocking(move || {
                filetime::set_file_mtime(&os, ft)
            })
            .await;
        }
    }

    Ok(written)
}
